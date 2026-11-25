import type { MessageKey } from "keycloakify/login/i18n/messages_defaultSet/types";

// Keycloakify has no built-in Korean bundle; theme translations live in i18n.ts and other Keycloak
// messages fall back to English.
const messages = {} as Record<MessageKey, string>;

export default messages;
