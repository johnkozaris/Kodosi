export interface AccountService {
  person(): Promise<Person>;
  save(values: Record<string, string>): Promise<void>;
  ways(): Promise<Way[]>;
  sessions(): Promise<Session[]>;
  endSession(id: string): Promise<void>;
  endOtherSessions(): Promise<void>;
  linked(): Promise<Linked[]>;
  unlink(alias: string): Promise<void>;
  programs(): Promise<Program[]>;
  revoke(clientId: string): Promise<void>;
  setLanguage(tag: string): Promise<void>;
  act(action: string): void;
  signOut(): void;
  came: { action: string; status: "success" | "cancelled" | "error" } | null;
}

export interface Field {
  name: string;
  label: string;
  value: string;
  required: boolean;
  readOnly: boolean;
}

export interface Person {
  username: string;
  email: string;
  emailStep: boolean;
  fields: Field[];
}

export interface Way {
  type: string;
  create?: string;
  update?: string;
  removable: boolean;
  held: Held[];
}

export interface Held {
  id: string;
  label?: string;
  created?: number;
  left?: number;
  total?: number;
}

export interface Session {
  id: string;
  browser: string;
  os: string;
  mobile: boolean;
  current: boolean;
  lastAccess: number;
  programs: string[];
}

export interface Linked {
  alias: string;
  name: string;
  providerId?: string;
  connected: boolean;
  as?: string;
}

export interface Program {
  clientId: string;
  name: string;
  stays: boolean;
}

export class Refused extends Error {
  constructor(
    readonly problems: Problem[],
    readonly fields: string[],
  ) {
    super("refused");
  }
}

export interface Problem {
  key: string;
  params: string[];
}

export class Unreachable extends Error {
  constructor(readonly denied = false) {
    super("unreachable");
  }
}
