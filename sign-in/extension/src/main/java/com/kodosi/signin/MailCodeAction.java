package com.kodosi.signin;

import jakarta.ws.rs.core.Response;
import org.keycloak.Config;
import org.keycloak.authentication.InitiatedActionSupport;
import org.keycloak.authentication.RequiredActionContext;
import org.keycloak.authentication.RequiredActionFactory;
import org.keycloak.authentication.RequiredActionProvider;
import org.keycloak.models.KeycloakSession;
import org.keycloak.models.KeycloakSessionFactory;
import org.keycloak.models.UserModel;
import org.keycloak.services.validation.Validation;

// Required action replacing Keycloak's built-in Verify Email with the Kodosi mail code flow.
public final class MailCodeAction implements RequiredActionProvider, RequiredActionFactory {
    static final String ID = "kodosi-mail-code";

    // Report SUPPORTED so a login specifying this pending action in kc_action cannot bypass it
    // (Keycloak CVE-2026-118959).
    @Override
    public InitiatedActionSupport initiatedActionSupport() {
        return InitiatedActionSupport.SUPPORTED;
    }

    @Override
    public void evaluateTriggers(RequiredActionContext context) {
        UserModel user = context.getUser();
        if (!context.getRealm().isVerifyEmail() || user.isEmailVerified() || Validation.isBlank(user.getEmail())) return;
        if (user.getRequiredActionsStream().anyMatch(UserModel.RequiredAction.UPDATE_EMAIL.name()::equals)) return;
        user.addRequiredAction(ID);
    }

    @Override
    public void requiredActionChallenge(RequiredActionContext context) {
        UserModel user = context.getUser();
        if (user.isEmailVerified()) {
            context.success();
            return;
        }
        if (Validation.isBlank(user.getEmail())) {
            context.ignore();
            return;
        }
        CodeStep step = step(context);
        if (context.getAuthenticationSession().getAuthNote(CodeStep.STARTED) != null) {
            context.challenge(step.show());
            return;
        }
        context.getAuthenticationSession().setAuthNote(CodeStep.STARTED, "true");
        context.challenge(step.start());
    }

    @Override
    public void processAction(RequiredActionContext context) {
        Response page = step(context).post(context.getHttpRequest().getDecodedFormParameters());
        if (page == null) context.success();
        else context.challenge(page);
    }

    private static CodeStep step(RequiredActionContext context) {
        return new CodeStep(context.getSession(), context.getAuthenticationSession(), context.getEvent(),
                context::form, CodeStep.Kind.ACCOUNT, context.getUser());
    }

    @Override public String getId() { return ID; }
    @Override public String getDisplayText() { return "Kodosi mail code"; }
    @Override public RequiredActionProvider create(KeycloakSession session) { return this; }
    @Override public void init(Config.Scope config) {}
    @Override public void postInit(KeycloakSessionFactory factory) {}
    @Override public void close() {}
}
