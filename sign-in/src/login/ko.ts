import type { MessageKey } from "keycloakify/login/i18n/messages_defaultSet/types";

// Keycloakify has no Korean words of its own. The theme's Korean words are in i18n.ts, and
// Keycloak's other words fall back to English.
const messages = {} as Record<MessageKey, string>;

export default messages;
