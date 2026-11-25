package com.kodosi.signin;

import jakarta.ws.rs.core.Response;
import java.util.List;
import org.jboss.logging.Logger;
import org.keycloak.Config;
import org.keycloak.authentication.AuthenticationFlowContext;
import org.keycloak.authentication.AuthenticationFlowError;
import org.keycloak.authentication.Authenticator;
import org.keycloak.authentication.AuthenticatorFactory;
import org.keycloak.models.AuthenticationExecutionModel.Requirement;
import org.keycloak.models.KeycloakSession;
import org.keycloak.models.KeycloakSessionFactory;
import org.keycloak.models.RealmModel;
import org.keycloak.models.UserModel;
import org.keycloak.provider.ProviderConfigProperty;
import org.keycloak.services.resources.LoginActionsService;
import org.keycloak.services.validation.Validation;
import org.keycloak.sessions.AuthenticationSessionModel;

public final class MailCodeAuthenticator implements Authenticator, AuthenticatorFactory {
    static final String ID = "kodosi-mail-code";
    private static final String ACCOUNT = "kodosi.code.account";
    private static final Logger LOG = Logger.getLogger(MailCodeAuthenticator.class);

    @Override
    public void authenticate(AuthenticationFlowContext context) {
        CodeStep.Kind kind = kindOf(context);
        if (kind == null) {
            LOG.errorf("%s works in the registration and the reset flow only", ID);
            context.failure(AuthenticationFlowError.INTERNAL_ERROR);
            return;
        }
        AuthenticationSessionModel authSession = context.getAuthenticationSession();
        if (authSession.getAuthNote(CodeStep.STARTED) != null) {
            context.challenge(step(context, kind).show());
            return;
        }
        authSession.setAuthNote(CodeStep.STARTED, "true");
        // Clear the flow user during reset so Keycloak does not alter the rendered page (e.g.
        // locale or header) and reveal whether the account exists; restore it once verified.
        if (kind == CodeStep.Kind.RESET) {
            UserModel user = context.getUser();
            if (user != null && user.isEnabled() && !Validation.isBlank(user.getEmail())) {
                authSession.setAuthNote(ACCOUNT, user.getId());
            }
            context.clearUser();
        }
        context.challenge(step(context, kind).start());
    }

    @Override
    public void action(AuthenticationFlowContext context) {
        CodeStep.Kind kind = kindOf(context);
        CodeStep step = step(context, kind);
        Response page = step.post(context.getHttpRequest().getDecodedFormParameters());
        if (page != null) {
            context.challenge(page);
            return;
        }
        if (kind == CodeStep.Kind.RESET) context.setUser(account(context));
        context.success();
    }

    private static CodeStep step(AuthenticationFlowContext context, CodeStep.Kind kind) {
        return new CodeStep(context.getSession(), context.getAuthenticationSession(), context.getEvent(),
                context::form, kind, kind == CodeStep.Kind.RESET ? account(context) : null);
    }

    private static UserModel account(AuthenticationFlowContext context) {
        String id = context.getAuthenticationSession().getAuthNote(ACCOUNT);
        UserModel user = id == null ? null : context.getSession().users().getUserById(context.getRealm(), id);
        return user != null && user.isEnabled() ? user : null;
    }

    private static CodeStep.Kind kindOf(AuthenticationFlowContext context) {
        return switch (context.getFlowPath()) {
            case LoginActionsService.REGISTRATION_PATH -> CodeStep.Kind.SIGN_UP;
            case LoginActionsService.RESET_CREDENTIALS_PATH -> CodeStep.Kind.RESET;
            case null, default -> null;
        };
    }

    @Override public boolean requiresUser() { return false; }
    @Override public boolean configuredFor(KeycloakSession session, RealmModel realm, UserModel user) { return true; }
    @Override public void setRequiredActions(KeycloakSession session, RealmModel realm, UserModel user) {}
    @Override public String getId() { return ID; }
    @Override public String getDisplayType() { return "Kodosi mail code"; }
    @Override public String getReferenceCategory() { return null; }
    @Override public boolean isConfigurable() { return false; }
    @Override public Requirement[] getRequirementChoices() { return new Requirement[] {Requirement.REQUIRED, Requirement.DISABLED}; }
    @Override public boolean isUserSetupAllowed() { return false; }

    @Override
    public String getHelpText() {
        return """
                Checks the mail address with a code: in the registration flow before the form, which then takes \
                the confirmed address, and in the reset flow in the place of the reset mail.""";
    }

    @Override public List<ProviderConfigProperty> getConfigProperties() { return List.of(); }
    @Override public Authenticator create(KeycloakSession session) { return this; }
    @Override public void init(Config.Scope config) {}
    @Override public void postInit(KeycloakSessionFactory factory) {}
    @Override public void close() {}
}
