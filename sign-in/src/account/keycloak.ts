import Keycloak from "keycloak-js";

import type { KcContext } from "./KcContext";
import {
  type AccountService,
  type Field,
  type Held,
  type Linked,
  type Person,
  type Problem,
  type Program,
  Refused,
  type Session,
  Unreachable,
  type Way,
} from "./service";

/** What Keycloak's account service sends: only what the page reads. */
interface RawPerson {
  username?: string;
  email?: string;
  firstName?: string;
  lastName?: string;
  attributes?: Record<string, string[]>;
  userProfileMetadata?: {
    attributes?: {
      name: string;
      displayName?: string;
      required?: boolean;
      readOnly?: boolean;
      multivalued?: boolean;
      annotations?: Record<string, unknown> | null;
    }[];
  };
  [more: string]: unknown;
}
interface RawWay {
  type: string;
  createAction?: string;
  updateAction?: string;
  removeable?: boolean;
  userCredentialMetadatas?: {
    credential: { id: string; userLabel?: string; createdDate?: number; credentialData?: string };
  }[];
}
interface RawDevice {
  os?: string;
  mobile?: boolean;
  sessions?: {
    id: string;
    lastAccess?: number;
    browser?: string;
    current?: boolean;
    clients?: { clientId: string; clientName?: string }[];
  }[];
}
interface RawLinked {
  providerAlias: string;
  displayName?: string;
  providerName?: string;
  connected?: boolean;
  linkedUsername?: string;
}
interface RawProgram {
  clientId: string;
  clientName?: string;
  offlineAccess?: boolean;
  consent?: unknown;
}
interface RawRefusal {
  field?: string;
  errorMessage?: string;
  params?: unknown[];
  errors?: RawRefusal[];
}

/** The details that Keycloak keeps beside the others, not among the person's attributes. */
const OWN = new Set(["username", "email", "firstName", "lastName"]);

/** Other systems read these names: "Other" and "Unknown" say that Keycloak found none. */
function known(name: string | undefined): string {
  return !name || /^(other|unknown)$/i.test(name) ? "" : name;
}

function refusal(data: RawRefusal | undefined): Refused | null {
  const said = (data?.errors?.length ? data.errors : data ? [data] : []).filter(
    (one) => one.errorMessage,
  );
  if (!said.length) return null;
  const problems: Problem[] = said.map((one) => ({
    key: one.errorMessage as string,
    params: (one.params ?? []).map(String),
  }));
  return new Refused(
    problems,
    said.flatMap((one) => (one.field ? [one.field] : [])),
  );
}

// keycloak-js makes a promise with a function that browsers have from 2024 (Safari 17.4). The
// pages work in older browsers than that, so those get the function here.
Promise.withResolvers ??= <T>() => {
  let resolve!: PromiseWithResolvers<T>["resolve"];
  let reject!: PromiseWithResolvers<T>["reject"];
  const promise = new Promise<T>((keep, refuse) => {
    resolve = keep;
    reject = refuse;
  });
  return { promise, resolve, reject };
};

let opened: Promise<AccountService> | undefined;

/**
 * Signs the page in to Keycloak's account service, with Keycloak's own client for the account
 * pages. A person with no session goes to the sign-in pages first, and comes back here.
 */
export function openAccount(kcContext: KcContext): Promise<AccountService> {
  // A sign-in that did not work is not kept: the next call tries again.
  opened ??= open(kcContext).catch((error: unknown) => {
    opened = undefined;
    throw error;
  });
  return opened;
}

async function open(kcContext: KcContext): Promise<AccountService> {
  const keycloak = new Keycloak({
    url: kcContext.authServerUrl,
    realm: kcContext.realm.name,
    clientId: kcContext.clientId,
  });
  let came: AccountService["came"] = null;
  keycloak.onActionUpdate = (status, action) => {
    came = { status, action: action ?? "" };
  };
  try {
    await keycloak.init({
      onLoad: "login-required",
      pkceMethod: "S256",
      checkLoginIframe: false,
      locale: kcContext.locale,
    });
  } catch {
    throw new Unreachable();
  }

  const base = `${kcContext.authServerUrl.replace(/\/+$/, "")}/realms/${encodeURIComponent(kcContext.realm.name)}/account`;
  /** The page leaves for the sign-in pages: nothing more happens on it. */
  const away = <T>() => new Promise<T>(() => {});

  async function call<T>(path: string, method = "GET", body?: unknown): Promise<T> {
    try {
      await keycloak.updateToken(5);
    } catch {
      // With the token still here, the request for a new one did not get through: the page says
      // so. With no token, Keycloak ended the session, and the person signs in again.
      if (keycloak.token) throw new Unreachable();
      void keycloak.login();
      return away();
    }
    let response: Response;
    try {
      response = await fetch(base + path, {
        method,
        headers: {
          Accept: "application/json",
          Authorization: `Bearer ${keycloak.token}`,
          ...(body === undefined ? {} : { "Content-Type": "application/json" }),
        },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      });
    } catch {
      throw new Unreachable();
    }
    if (response.status === 401) {
      void keycloak.login();
      return away();
    }
    const text = await response.text();
    let data: unknown;
    try {
      data = text ? JSON.parse(text) : undefined;
    } catch {
      if (response.ok) throw new Unreachable();
    }
    if (!response.ok)
      throw (
        refusal(data as RawRefusal) ??
        new Unreachable(response.status === 403 || response.status === 404)
      );
    return data as T;
  }

  /** A list that the realm can keep from a person: the page then has no such group. */
  const optional = <T>(list: Promise<T[]>): Promise<T[]> =>
    list.catch((error: unknown) => {
      if (error instanceof Unreachable && error.denied) return [];
      throw error;
    });

  // Keycloak takes the whole record. A save reads the record first, so it does not put an old
  // value over a change from a different tab. It starts when the save before it has ended.
  let saved: Promise<unknown> = Promise.resolve();
  const save: AccountService["save"] = (values) => {
    const next = saved
      .catch(() => {})
      .then(async () => {
        const { userProfileMetadata: _, ...record } = await call<RawPerson>(
          "/?userProfileMetadata=false",
        );
        const attributes = { ...record.attributes };
        for (const [name, value] of Object.entries(values)) {
          if (OWN.has(name)) record[name] = value;
          else attributes[name] = [value];
        }
        await call("/", "POST", { ...record, attributes });
      });
    saved = next;
    return next;
  };

  return {
    came,

    async person(): Promise<Person> {
      const record = await call<RawPerson>("/?userProfileMetadata=true");
      const fields: Field[] = (record.userProfileMetadata?.attributes ?? []).map((one) => ({
        name: one.name,
        label: one.displayName || one.name,
        value: OWN.has(one.name)
          ? String(record[one.name] ?? "")
          : (record.attributes?.[one.name]?.[0] ?? ""),
        required: !!one.required,
        // A row holds one value, so a detail with more than one value has no row that changes it.
        readOnly: !!one.readOnly || !!one.multivalued,
      }));
      // Keycloak marks the email when its change is a step that this person can take. It leaves
      // the mark out where the email is the name of the sign-in and names do not change.
      const emailStep = !!record.userProfileMetadata?.attributes?.find(
        (one) => one.name === "email",
      )?.annotations?.["kc.required.action.supported"];
      return { username: record.username ?? "", email: record.email ?? "", emailStep, fields };
    },

    save,

    async ways(): Promise<Way[]> {
      const raw = await call<RawWay[]>("/credentials");
      return raw.map((way) => ({
        type: way.type,
        ...(way.createAction ? { create: way.createAction } : {}),
        ...(way.updateAction ? { update: way.updateAction } : {}),
        removable: !!way.removeable,
        held: (way.userCredentialMetadatas ?? []).map(({ credential }): Held => {
          let codes: { remainingCodes?: unknown; totalCodes?: unknown } = {};
          try {
            codes = JSON.parse(credential.credentialData ?? "{}") ?? {};
          } catch {
            // The row shows no count.
          }
          // For a person that a different user store holds, Keycloak has no day: it sends -1.
          const created = credential.createdDate ?? 0;
          return {
            id: credential.id,
            ...(credential.userLabel ? { label: credential.userLabel } : {}),
            ...(created > 0 ? { created } : {}),
            ...(typeof codes.remainingCodes === "number" ? { left: codes.remainingCodes } : {}),
            ...(typeof codes.totalCodes === "number" ? { total: codes.totalCodes } : {}),
          };
        }),
      }));
    },

    async sessions(): Promise<Session[]> {
      const raw = await call<RawDevice[]>("/sessions/devices");
      // Keycloak can list one sign-in two times, as a session and as its offline twin. It is one
      // row here, because a sign-out of the one ends the other.
      const sessions = new Map<string, Session>();
      for (const device of raw) {
        for (const session of device.sessions ?? []) {
          const twin = sessions.get(session.id);
          // The account page itself is no program that a person signed in to.
          const programs = (session.clients ?? [])
            .filter((one) => one.clientId !== kcContext.clientId)
            .map((one) => one.clientName || one.clientId);
          sessions.set(session.id, {
            id: session.id,
            browser: known(session.browser?.split("/")[0]),
            os: known(device.os),
            mobile: !!device.mobile,
            current: !!session.current || !!twin?.current,
            lastAccess: Math.max((session.lastAccess ?? 0) * 1000, twin?.lastAccess ?? 0),
            programs: [...new Set([...(twin?.programs ?? []), ...programs])],
          });
        }
      }
      return [...sessions.values()].toSorted(
        (a, b) => Number(b.current) - Number(a.current) || b.lastAccess - a.lastAccess,
      );
    },
    endSession: (id) => call(`/sessions/${encodeURIComponent(id)}`, "DELETE"),
    endOtherSessions: () => call("/sessions?current=false", "DELETE"),

    async linked(): Promise<Linked[]> {
      if (!kcContext.isLinkedAccountsEnabled) return [];
      const raw = await optional(call<RawLinked[]>("/linked-accounts"));
      return raw.map((one) => ({
        alias: one.providerAlias,
        name: one.displayName || one.providerName || one.providerAlias,
        ...(one.providerName ? { providerId: one.providerName } : {}),
        connected: !!one.connected,
        ...(one.linkedUsername ? { as: one.linkedUsername } : {}),
      }));
    },
    unlink: (alias) => call(`/linked-accounts/${encodeURIComponent(alias)}`, "DELETE"),

    async programs(): Promise<Program[]> {
      if (kcContext.isViewApplicationsEnabled === false) return [];
      const raw = await optional(call<RawProgram[]>("/applications"));
      // A program is here only when the person can take something back from it.
      return raw
        .filter((one) => one.consent || one.offlineAccess)
        .map((one) => ({
          clientId: one.clientId,
          name: one.clientName || one.clientId,
          stays: !!one.offlineAccess,
        }));
    },
    revoke: (clientId) => call(`/applications/${encodeURIComponent(clientId)}/consent`, "DELETE"),

    async setLanguage(tag) {
      await save({ locale: tag });
      location.reload();
      return away();
    },

    act(action) {
      void keycloak.login({ action });
    },
    signOut() {
      void keycloak.logout({ redirectUri: location.origin + kcContext.baseUrl.path });
    },
  };
}
