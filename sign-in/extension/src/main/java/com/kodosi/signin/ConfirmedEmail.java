package com.kodosi.signin;

import jakarta.ws.rs.core.MultivaluedMap;
import java.util.List;
import org.keycloak.Config;
import org.keycloak.authentication.FormAction;
import org.keycloak.authentication.FormActionFactory;
import org.keycloak.authentication.FormContext;
import org.keycloak.authentication.ValidationContext;
import org.keycloak.events.Errors;
import org.keycloak.forms.login.LoginFormsProvider;
import org.keycloak.models.AuthenticationExecutionModel.Requirement;
import org.keycloak.models.KeycloakSession;
import org.keycloak.models.KeycloakSessionFactory;
import org.keycloak.models.RealmModel;
import org.keycloak.models.UserModel;
import org.keycloak.models.utils.FormMessage;
import org.keycloak.provider.ProviderConfigProperty;

// Locks registration to the email verified by CodeStep; must run after Registration User Profile
// Creation so context.getUser() exists in success().
public final class ConfirmedEmail implements FormAction, FormActionFactory {
    static final String ID = "kodosi-confirmed-email";

    @Override
    public void buildPage(FormContext context, LoginFormsProvider form) {
        form.setAttribute("kodosiEmail", confirmed(context));
    }

    @Override
    public void validate(ValidationContext context) {
        MultivaluedMap<String, String> form = context.getHttpRequest().getDecodedFormParameters();
        String confirmed = confirmed(context);
        String given = form.getFirst(UserModel.EMAIL);
        if (confirmed != null && given != null && confirmed.equalsIgnoreCase(given.trim())) {
            context.success();
            return;
        }
        // Suppress Keycloak's built-in validation errors so a tampered email field cannot be used
        // to probe whether another account owns that address.
        context.error(Errors.INVALID_REGISTRATION);
        context.validationError(form, List.of(new FormMessage(null, "kdsBadEmail")));
        context.excludeOtherErrors();
    }

    @Override
    public void success(FormContext context) {
        context.getUser().setEmailVerified(true);
    }

    private static String confirmed(FormContext context) {
        return context.getAuthenticationSession().getAuthNote(CodeStep.CONFIRMED);
    }

    @Override public boolean requiresUser() { return false; }
    @Override public boolean configuredFor(KeycloakSession session, RealmModel realm, UserModel user) { return true; }
    @Override public void setRequiredActions(KeycloakSession session, RealmModel realm, UserModel user) {}
    @Override public String getId() { return ID; }
    @Override public String getDisplayType() { return "Kodosi confirmed email"; }
    @Override public String getReferenceCategory() { return null; }
    @Override public boolean isConfigurable() { return false; }
    @Override public Requirement[] getRequirementChoices() { return new Requirement[] {Requirement.REQUIRED, Requirement.DISABLED}; }
    @Override public boolean isUserSetupAllowed() { return false; }

    @Override
    public String getHelpText() {
        return """
                Fixes the address of the registration form to the one that the Kodosi mail code confirmed. Put it \
                after Registration User Profile Creation.""";
    }

    @Override public List<ProviderConfigProperty> getConfigProperties() { return List.of(); }
    @Override public FormAction create(KeycloakSession session) { return this; }
    @Override public void init(Config.Scope config) {}
    @Override public void postInit(KeycloakSessionFactory factory) {}
    @Override public void close() {}
}
