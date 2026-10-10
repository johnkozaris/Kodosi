import type { KcContext } from "./KcContext";
import type { Problem } from "./service";

// The words of the account page. {0} and {1} are values. The apps have English only, so the page
// has English only: for a sentence of Keycloak that has no words here, Keycloak's own words show.
const en = {
  title: "Your account",
  signIn: "Sign-in",
  signedIn: "Where you are signed in",
  details: "Your details",
  linked: "Linked accounts",
  apps: "Apps with access",

  password: "Password",
  passwordSet: "Set a password",
  changed: "Changed {0}",
  added: "Added {0}",
  passkey: "Passkey",
  passkeyAdd: "Add a passkey",
  passkeyHint: "Your fingerprint, your face or your PIN. No password.",
  authenticator: "Authenticator app",
  authenticatorAdd: "Add an authenticator app",
  authenticatorHint: "A code from an app on your phone, for each sign-in.",
  securityKey: "Security key",
  securityKeyAdd: "Add a security key",
  recovery: "Recovery codes",
  recoveryHint: "For a sign-in when you do not have your phone.",
  recoveryLeft: "{0} of {1} left",
  recoveryAgain: "New codes",
  remove: "Remove {0}",

  thisBrowser: "This browser",
  now: "Active now",
  browserOn: "{0} on {1}",
  browserUnknown: "A browser",
  signOutOne: "Sign out",
  signOutOf: "Sign out: {0}",
  signOutOthers: "Sign out everywhere else",
  staysSignedIn: "Stays signed in on your computers",

  addOne: "Add",
  addNamed: "Add {0}",
  removeOne: "Remove",

  firstName: "First name",
  lastName: "Last name",
  email: "Email",
  username: "Username",
  saved: "Saved",
  notSaved: "Not saved",

  backTo: "Back to {0}",
  signOut: "Sign out",
  language: "Language",
  deleteAccount: "Delete my account",

  unreachable: "Your account did not open.",
  tryAgain: "Try again",
  failed: "That did not work.",
  required: "Fill in this row.",
  invalidEmail: "That email is not right.",
  badLength: "That is too long or too short.",
  badName: "A name cannot have that character.",
  emailTaken: "A different account has that email.",
  usernameTaken: "That username is taken. Pick a different one.",
  readOnly: "You cannot change this here.",
};

export type WordKey = keyof typeof en;

// Keycloak's keys for what it refuses, and the words of the page for them.
const PROBLEMS: Record<string, WordKey> = {
  "error-user-attribute-required": "required",
  "error-invalid-blank": "required",
  "error-empty": "required",
  "error-invalid-email": "invalidEmail",
  invalidEmailMessage: "invalidEmail",
  "error-invalid-length": "badLength",
  "error-invalid-length-too-long": "badLength",
  "error-invalid-length-too-short": "badLength",
  "error-person-name-invalid-character": "badName",
  "error-username-invalid-character": "badName",
  emailExistsMessage: "emailTaken",
  usernameExistsMessage: "usernameTaken",
  "error-user-attribute-read-only": "readOnly",
  readOnlyUserMessage: "readOnly",
  readOnlyUsernameMessage: "readOnly",
  updateReadOnlyAttributesRejectedMessage: "readOnly",
};

const DAY = 24 * 3_600_000;

function fill(text: string, values: (string | number)[]): string {
  return text.replace(/\{(\d)\}/g, (_, i) => `${values[Number(i)] ?? ""}`);
}

export interface Words {
  say(key: WordKey, ...values: (string | number)[]): string;
  /** A day, as "12 March". */
  day(time: number): string;
  /** How long ago, as "3 hours ago". */
  ago(time: number): string;
  /** The name of a detail: the page's own word, or Keycloak's. */
  label(name: string, label: string): string;
  /** What Keycloak said about a refused change, in the words of the page. */
  problem(problem: Problem): string;
  /** Keycloak's name of a thing, when the name is a key of Keycloak's messages. */
  named(name: string): string;
  languageName(tag: string): string;
}

export function wordsOf(kcContext: Pick<KcContext, "locale" | "msgJSON">): Words {
  let keycloak: Record<string, string> = {};
  try {
    keycloak = JSON.parse(kcContext.msgJSON ?? "{}");
  } catch {
    // The page keeps its own words.
  }
  // The words are English, so the days and the times are English too.
  const date = new Intl.DateTimeFormat("en", { day: "numeric", month: "long" });
  const dateOfYear = new Intl.DateTimeFormat("en", { dateStyle: "long" });
  const relative = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

  const say: Words["say"] = (key, ...values) => fill(en[key], values);
  /** `${firstName}` is the key `firstName`. */
  const named: Words["named"] = (name) => {
    const key = /^\$\{(.+)\}$/.exec(name)?.[1];
    return key ? (keycloak[key] ?? key) : name;
  };
  return {
    say,
    named,
    // A day of this year needs no year.
    day: (time) =>
      new Date(time).getFullYear() === new Date().getFullYear()
        ? date.format(time)
        : dateOfYear.format(time),
    ago(time) {
      const passed = Date.now() - time;
      // Whole units that passed: 59 minutes and a half are 59 minutes, not 60.
      if (passed < 3_600_000)
        return relative.format(-Math.max(1, Math.floor(passed / 60_000)), "minute");
      if (passed < DAY) return relative.format(-Math.floor(passed / 3_600_000), "hour");
      return relative.format(-Math.floor(passed / DAY), "day");
    },
    label(name, label) {
      if (name === "firstName" || name === "lastName" || name === "email" || name === "username")
        return en[name];
      return named(label);
    },
    problem({ key, params }) {
      const mine = PROBLEMS[key];
      if (mine) return en[mine];
      const theirs = keycloak[key];
      // For some refusals Keycloak sends no key: it sends the sentence.
      if (!theirs) return /\s/.test(key) ? key : en.failed;
      // Keycloak writes a value as {{param_0}} in these words.
      return theirs.replace(/\{\{param_(\d)\}\}/g, (_, i) => named(params[Number(i)] ?? ""));
    },
    languageName: (tag) => new Intl.DisplayNames([tag], { type: "language" }).of(tag) ?? tag,
  };
}
