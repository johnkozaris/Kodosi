/**
 * What the account page asks of Keycloak. Keycloak holds each fact and does each change: the
 * page shows them. A step that needs the person's proof (a new password, a passkey, a removal,
 * a link to a different sign-in service) is a step of the sign-in pages, and the person comes
 * back here after it.
 */
export interface AccountService {
  person(): Promise<Person>;
  /** Saves the details that the person can change. It throws `Refused` for what Keycloak refuses. */
  save(values: Record<string, string>): Promise<void>;
  ways(): Promise<Way[]>;
  sessions(): Promise<Session[]>;
  endSession(id: string): Promise<void>;
  endOtherSessions(): Promise<void>;
  linked(): Promise<Linked[]>;
  unlink(alias: string): Promise<void>;
  programs(): Promise<Program[]>;
  revoke(clientId: string): Promise<void>;
  /** Keeps the language for the person. The page then loads again in it. */
  setLanguage(tag: string): Promise<void>;
  /** Goes to a step of the sign-in pages. The page loads again when the step ends. */
  act(action: string): void;
  signOut(): void;
  /** The step that the person came back from, and how it ended. */
  came: { action: string; status: "success" | "cancelled" | "error" } | null;
}

/** A detail of the person: a row of their profile in the realm. */
export interface Field {
  name: string;
  /** Keycloak's name for the row: words, or a key of its messages as `${firstName}`. */
  label: string;
  value: string;
  required: boolean;
  readOnly: boolean;
}

export interface Person {
  username: string;
  email: string;
  /** The realm changes an email in a step of the sign-in pages, and this person can take it. */
  emailStep: boolean;
  fields: Field[];
}

/** A way of sign-in that the realm offers, with what the person has of it. */
export interface Way {
  /** Keycloak's kind: "password", "otp", "webauthn-passwordless", "recovery-authn-codes". */
  type: string;
  /** The step that adds one. */
  create?: string;
  /** The step that replaces the one that the person has. */
  update?: string;
  removable: boolean;
  held: Held[];
}

export interface Held {
  id: string;
  /** The name that the person gave it. */
  label?: string;
  /** When it was made, in milliseconds. */
  created?: number;
  /** Recovery codes: how many of them are not used yet, and how many there were. */
  left?: number;
  total?: number;
}

/** One sign-in of the person that is open in a browser. */
export interface Session {
  id: string;
  browser: string;
  os: string;
  mobile: boolean;
  /** This page runs in it. */
  current: boolean;
  /** When it last did something, in milliseconds. */
  lastAccess: number;
  /** The programs that the person signed in to from it: the Kodosi app, for example. */
  programs: string[];
}

/** A sign-in service of a different company, which can be joined to this account. */
export interface Linked {
  alias: string;
  name: string;
  /** Keycloak's kind of the service: "github", "google". */
  providerId?: string;
  connected: boolean;
  /** The person's name at that service. */
  as?: string;
}

/** A program that the person let use their account. */
export interface Program {
  clientId: string;
  name: string;
  /** The program stays signed in when the person closes the browser, as the Kodosi app does. */
  stays: boolean;
}

/** Keycloak refused a change. `fields` has the rows that it names. */
export class Refused extends Error {
  constructor(
    readonly problems: Problem[],
    readonly fields: string[],
  ) {
    super("refused");
  }
}

/** A sentence of Keycloak as a key of its messages, with the values that it holds. */
export interface Problem {
  key: string;
  params: string[];
}

/** The account service did not answer, or it answered with an error. */
export class Unreachable extends Error {
  /** `denied`: Keycloak answered that this person does not get the thing. */
  constructor(readonly denied = false) {
    super("unreachable");
  }
}
