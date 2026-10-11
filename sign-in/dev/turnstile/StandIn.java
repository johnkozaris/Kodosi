package kodosi.signin;

import jakarta.ws.rs.core.MultivaluedMap;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Proxy;
import java.net.URI;
import java.net.URLEncoder;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;
import java.time.Duration;
import java.util.List;
import java.util.Map;
import java.util.function.Consumer;
import org.keycloak.Config;
import org.keycloak.authentication.AuthenticationFlowContext;
import org.keycloak.authentication.Authenticator;
import org.keycloak.authentication.FormAction;
import org.keycloak.authentication.FormActionFactory;
import org.keycloak.authentication.FormContext;
import org.keycloak.authentication.ValidationContext;
import org.keycloak.authentication.authenticators.resetcred.ResetCredentialChooseUser;
import org.keycloak.events.Errors;
import org.keycloak.forms.login.LoginFormsProvider;
import org.keycloak.models.AuthenticationExecutionModel.Requirement;
import org.keycloak.models.AuthenticatorConfigModel;
import org.keycloak.models.KeycloakSession;
import org.keycloak.models.KeycloakSessionFactory;
import org.keycloak.models.RealmModel;
import org.keycloak.models.UserModel;
import org.keycloak.models.utils.FormMessage;
import org.keycloak.provider.ProviderConfigProperty;

/**
 * A stand-in for a Cloudflare Turnstile extension, for the disposable Keycloak of keycloak.sh only.
 * It gives the pages what github.com/zymlabs/keycloak-cloudflare-turnstile-provider gives them:
 * on the registration form the values turnstileRequired, turnstileSiteKey, turnstileMode and
 * turnstileTheme, on the reset form the two scripts of its injection, and on both the check of the
 * field cf-turnstile-response. Use it with Cloudflare's test keys only.
 */
public final class StandIn {
    private static final String FIELD = "cf-turnstile-response";
    private static final String VERIFY = "https://challenges.cloudflare.com/turnstile/v0/siteverify";
    private static final HttpClient HTTP = HttpClient.newBuilder().connectTimeout(Duration.ofSeconds(10)).build();
    private static final List<ProviderConfigProperty> KEYS = List.of(
            new ProviderConfigProperty("site.key", "Site key", "A Cloudflare test site key.",
                    ProviderConfigProperty.STRING_TYPE, "1x00000000000000000000AA"),
            new ProviderConfigProperty("secret.key", "Secret key", "A Cloudflare test secret key.",
                    ProviderConfigProperty.STRING_TYPE, "1x0000000000000000000000000000000AA"));

    private StandIn() {}

    private static String key(AuthenticatorConfigModel config, String name) {
        Map<String, String> values = config == null ? Map.of() : config.getConfig();
        String value = values.get(name);
        if (value != null && !value.isBlank()) return value;
        return (String) KEYS.stream().filter(one -> one.getName().equals(name)).findFirst().orElseThrow().getDefaultValue();
    }

    private static String encoded(String value) {
        return URLEncoder.encode(value, StandardCharsets.UTF_8);
    }

    /** Asks Cloudflare whether the token of the widget is good. */
    private static boolean passes(AuthenticatorConfigModel config, MultivaluedMap<String, String> form, String address) {
        String token = form.getFirst(FIELD);
        if (token == null || token.isBlank()) return false;
        String body = "secret=" + encoded(key(config, "secret.key")) + "&response=" + encoded(token)
                + (address == null ? "" : "&remoteip=" + encoded(address));
        HttpRequest request = HttpRequest.newBuilder(URI.create(VERIFY))
                .timeout(Duration.ofSeconds(10))
                .header("Content-Type", "application/x-www-form-urlencoded")
                .POST(HttpRequest.BodyPublishers.ofString(body))
                .build();
        try {
            String answer = HTTP.send(request, HttpResponse.BodyHandlers.ofString()).body();
            return answer.replaceAll("\\s", "").contains("\"success\":true");
        } catch (Exception e) {
            return false;
        }
    }

    /** The registration form, with the values that a theme reads to draw the widget. */
    public static final class Registration implements FormAction, FormActionFactory {
        @Override
        public void buildPage(FormContext context, LoginFormsProvider form) {
            form.setAttribute("turnstileRequired", true)
                    .setAttribute("turnstileSkipped", false)
                    .setAttribute("turnstileSiteKey", key(context.getAuthenticatorConfig(), "site.key"))
                    .setAttribute("turnstileMode", "managed")
                    .setAttribute("turnstileTheme", "auto");
        }

        @Override
        public void validate(ValidationContext context) {
            MultivaluedMap<String, String> form = context.getHttpRequest().getDecodedFormParameters();
            if (passes(context.getAuthenticatorConfig(), form, context.getConnection().getRemoteAddr())) {
                context.success();
                return;
            }
            context.error(Errors.INVALID_REGISTRATION);
            String key = form.getFirst(FIELD) == null || form.getFirst(FIELD).isBlank()
                    ? "turnstileMissingToken" : "turnstileVerificationFailed";
            context.validationError(form, List.of(new FormMessage(null, key)));
            context.excludeOtherErrors();
        }

        @Override public void success(FormContext context) {}
        @Override public boolean requiresUser() { return false; }
        @Override public boolean configuredFor(KeycloakSession session, RealmModel realm, UserModel user) { return true; }
        @Override public void setRequiredActions(KeycloakSession session, RealmModel realm, UserModel user) {}
        @Override public String getId() { return "kodosi-turnstile-registration"; }
        @Override public String getDisplayType() { return "Turnstile stand-in"; }
        @Override public String getReferenceCategory() { return null; }
        @Override public boolean isConfigurable() { return true; }
        @Override public Requirement[] getRequirementChoices() { return new Requirement[] {Requirement.REQUIRED, Requirement.DISABLED}; }
        @Override public boolean isUserSetupAllowed() { return false; }
        @Override public String getHelpText() { return "A Turnstile check with Cloudflare's test keys, for a local look at the pages."; }
        @Override public List<ProviderConfigProperty> getConfigProperties() { return KEYS; }
        @Override public FormAction create(KeycloakSession session) { return this; }
        @Override public void init(Config.Scope config) {}
        @Override public void postInit(KeycloakSessionFactory factory) {}
        @Override public void close() {}
    }

    /** Keycloak's "choose user" step of the reset form, with the scripts of the injection on its page. */
    public static final class Reset extends ResetCredentialChooseUser {
        private static AuthenticationFlowContext withScripts(AuthenticationFlowContext context) {
            String key = key(context.getAuthenticatorConfig(), "site.key");
            String base = context.getUriInfo().getBaseUri().getPath().replaceAll("/$", "") + "/realms/"
                    + context.getRealm().getName() + "/kodosi-turnstile";
            Consumer<LoginFormsProvider> scripts = form -> {
                form.addScript(base + "/config.js?siteKey=" + encoded(key) + "&mode=managed&theme=auto&debug=false");
                form.addScript(base + "/resources/js/turnstile-injector.js");
            };
            boolean[] added = {false};
            return (AuthenticationFlowContext) Proxy.newProxyInstance(AuthenticationFlowContext.class.getClassLoader(),
                    new Class<?>[] {AuthenticationFlowContext.class}, (proxy, method, args) -> {
                        Object result;
                        try {
                            result = method.invoke(context, args);
                        } catch (InvocationTargetException e) {
                            throw e.getCause();
                        }
                        if (!added[0] && result instanceof LoginFormsProvider form && "form".equals(method.getName())) {
                            added[0] = true;
                            scripts.accept(form);
                        }
                        return result;
                    });
        }

        @Override
        public void authenticate(AuthenticationFlowContext context) {
            super.authenticate(withScripts(context));
        }

        @Override
        public void action(AuthenticationFlowContext context) {
            MultivaluedMap<String, String> form = context.getHttpRequest().getDecodedFormParameters();
            if (passes(context.getAuthenticatorConfig(), form, context.getConnection().getRemoteAddr())) {
                super.action(withScripts(context));
                return;
            }
            context.getEvent().error(Errors.INVALID_REQUEST);
            String key = form.getFirst(FIELD) == null || form.getFirst(FIELD).isBlank()
                    ? "turnstileMissingToken" : "turnstileVerificationFailed";
            context.forceChallenge(withScripts(context).form().setError(key).createPasswordReset());
        }

        @Override public String getId() { return "kodosi-turnstile-reset"; }
        @Override public String getDisplayType() { return "Choose user, with a Turnstile stand-in"; }
        @Override public boolean isConfigurable() { return true; }
        @Override public List<ProviderConfigProperty> getConfigProperties() { return KEYS; }
        @Override public Authenticator create(KeycloakSession session) { return this; }
    }
}
