package com.kodosi.signin;

import static java.nio.charset.StandardCharsets.UTF_8;

import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.security.SecureRandom;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HexFormat;
import java.util.List;
import java.util.Locale;

// Manages email verification codes for a single auth session. Codes live in session notes keyed by
// address so failed attempts in other sessions cannot invalidate this user's codes; the shared
// Store atomically caps per-address sends and per-session checks even across concurrent requests.
final class MailCode {
    static final int DIGITS = 8;
    static final int RESEND_SECONDS = 60;
    private static final int KEPT = 3;
    private static final int WRONG_PER_SIGN_IN = 5;
    private static final int MAILS_PER_HOUR = 5;
    private static final int COMPARED_PER_SIGN_IN = WRONG_PER_SIGN_IN + MAILS_PER_HOUR;
    private static final int HOUR = 3600;
    private static final int DAY = 86400;

    private static final String CODES = "kodosi.code.codes";
    private static final String SENT = "kodosi.code.sent";
    private static final String LAST = "kodosi.code.last";
    private static final String WRONG = "kodosi.code.wrong";
    private static final String SIGN_IN = "kodosi.code.signIn";

    private static final SecureRandom RANDOM = new SecureRandom();

    interface Notes {
        String get(String name);

        void set(String name, String value);

        void remove(String name);
    }

    // Atomic TTL-backed key store (hold succeeds only if the key is absent).
    interface Store {
        boolean hold(String key, int seconds);

        void drop(String key);
    }

    enum Shown { SENT, WAIT, LATER }

    enum Mail { NONE, CODE, ACCOUNT }

    record Send(Shown shown, Mail mail, String code) {}

    enum Check { PASSED, WRONG, ENDED, OLD }

    private final Notes notes;
    private final Store store;
    private final long now;
    private final int life;

    MailCode(Notes notes, Store store, long now, int life) {
        this.notes = notes;
        this.store = store;
        this.now = now;
        this.life = life;
    }

    // address may be null (e.g. reset for an unknown account) to apply the same pacing without
    // sending mail; when codeWorks is false, sends an existing-account notice instead of a code.
    Send send(String address, boolean codeWorks) {
        if (waitSeconds() > 0) return new Send(Shown.WAIT, Mail.NONE, null);
        List<Long> sent = sent();
        if (sent.size() >= MAILS_PER_HOUR) return new Send(Shown.LATER, Mail.NONE, null);
        sent.add(now);
        notes.set(SENT, join(sent));
        notes.set(LAST, Long.toString(now));
        if (address == null || hold("mail", key(address), MAILS_PER_HOUR, HOUR) == null) {
            return new Send(Shown.SENT, Mail.NONE, null);
        }
        if (!codeWorks) return new Send(Shown.SENT, Mail.ACCOUNT, null);
        String code = String.format("%0" + DIGITS + "d", RANDOM.nextInt((int) Math.pow(10, DIGITS)));
        signIn();
        List<String> codes = new ArrayList<>(codes(address));
        codes.add(code + "@" + now);
        notes.set(codesOf(address), String.join(",", codes.subList(Math.max(0, codes.size() - KEPT), codes.size())));
        return new Send(Shown.SENT, Mail.CODE, code);
    }

    Check check(String typed, String address) {
        String digits = typed == null ? "" : typed.replaceAll("[\\s-]", "");
        if (!digits.matches("[0-9]{" + DIGITS + "}")) return Check.WRONG;
        String held = address == null || codes(address).isEmpty()
                ? null
                : hold("compared", signIn() + "." + key(address), COMPARED_PER_SIGN_IN, DAY);
        if (held != null) {
            for (String entry : codes(address)) {
                int at = entry.indexOf('@');
                if (!MessageDigest.isEqual(entry.substring(0, at).getBytes(UTF_8), digits.getBytes(UTF_8))) continue;
                store.drop(held);
                if (number(entry.substring(at + 1)) + life <= now) return Check.OLD;
                notes.remove(codesOf(address));
                return Check.PASSED;
            }
        }
        long wrong = number(notes.get(WRONG)) + 1;
        notes.set(WRONG, Long.toString(wrong));
        if (wrong < WRONG_PER_SIGN_IN) return Check.WRONG;
        if (address != null) notes.remove(codesOf(address));
        return Check.ENDED;
    }

    void forget() {
        notes.remove(LAST);
    }

    int waitSeconds() {
        String last = notes.get(LAST);
        return last == null ? 0 : Math.clamp(number(last) + RESEND_SECONDS - now, 0, RESEND_SECONDS);
    }

    boolean tooOften() {
        return sent().size() >= MAILS_PER_HOUR;
    }

    private List<String> codes(String address) {
        String codes = notes.get(codesOf(address));
        return codes == null || codes.isEmpty() ? List.of() : List.of(codes.split(","));
    }

    private static String codesOf(String address) {
        return CODES + "." + key(address);
    }

    private List<Long> sent() {
        String sent = notes.get(SENT);
        List<Long> times = new ArrayList<>();
        if (sent == null || sent.isEmpty()) return times;
        Arrays.stream(sent.split(",")).map(MailCode::number).filter(time -> time > now - HOUR).forEach(times::add);
        return times;
    }

    private String signIn() {
        String id = notes.get(SIGN_IN);
        if (id == null) {
            byte[] random = new byte[16];
            RANDOM.nextBytes(random);
            id = HexFormat.of().formatHex(random);
            notes.set(SIGN_IN, id);
        }
        return id;
    }

    private String hold(String kind, String who, int limit, int seconds) {
        String prefix = "kodosi-mail-code." + kind + "." + who + ".";
        for (int slot = 0; slot < limit; slot++) {
            if (store.hold(prefix + slot, seconds)) return prefix + slot;
        }
        return null;
    }

    private static String key(String address) {
        try {
            byte[] text = address.toLowerCase(Locale.ROOT).getBytes(UTF_8);
            return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(text));
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
    }

    private static long number(String text) {
        return text == null ? 0 : Long.parseLong(text);
    }

    private static String join(List<Long> times) {
        return String.join(",", times.stream().map(String::valueOf).toList());
    }
}
