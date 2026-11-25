package com.kodosi.signin;

import jakarta.ws.rs.GET;
import jakarta.ws.rs.Produces;
import jakarta.ws.rs.core.CacheControl;
import jakarta.ws.rs.core.MediaType;
import jakarta.ws.rs.core.Response;
import jakarta.ws.rs.ext.Provider;
import java.util.Map;
import org.keycloak.Config;
import org.keycloak.common.util.Time;
import org.keycloak.models.KeycloakSession;
import org.keycloak.models.KeycloakSessionFactory;
import org.keycloak.services.resource.RealmResourceProvider;
import org.keycloak.services.resource.RealmResourceProviderFactory;

// Exposes Keycloak's current time to the account page: DPoP proofs require clock alignment within
// seconds, and Keycloak responses do not expose a readable Date header.
public final class ServerTime implements RealmResourceProvider, RealmResourceProviderFactory {
    static final String ID = "kodosi-time";

    // Keycloak only exposes realm resource endpoints on a @Provider class packaged with
    // META-INF/beans.xml.
    @Provider
    public static final class Now {
        @GET
        @Produces(MediaType.APPLICATION_JSON)
        public Response get() {
            CacheControl never = new CacheControl();
            never.setNoStore(true);
            return Response.ok(Map.of("now", Time.currentTimeMillis())).cacheControl(never).build();
        }
    }

    @Override public Object getResource() { return new Now(); }
    @Override public RealmResourceProvider create(KeycloakSession session) { return this; }
    @Override public String getId() { return ID; }
    @Override public void init(Config.Scope config) {}
    @Override public void postInit(KeycloakSessionFactory factory) {}
    @Override public void close() {}
}
