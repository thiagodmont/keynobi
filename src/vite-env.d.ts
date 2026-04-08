/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Sentry browser DSN — inject at build via CI secret; never commit. */
  readonly VITE_SENTRY_DSN?: string;
  /** App version from `package.json` (see `vite.config.ts` define). */
  readonly VITE_APP_VERSION: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
