import { commands, type Settings } from '@/bindings';

export * from '@/bindings';

/** Settings with every field present (Rust fills defaults on load). */
export type DeepRequired<T> = T extends (infer U)[]
  ? DeepRequired<U>[]
  : T extends object
    ? { [K in keyof T]-?: DeepRequired<Exclude<T[K], undefined>> }
    : T;

export type FullSettings = DeepRequired<Settings>;

type Res<T> = { status: 'ok'; data: T } | { status: 'error'; error: string };

/** Unwrap a typed command result, throwing the error string. */
export async function unwrap<T>(p: Promise<Res<T>>): Promise<T> {
  const r = await p;
  if (r.status === 'error') throw new Error(r.error);
  return r.data;
}

export { commands };
