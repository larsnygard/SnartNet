/* tslint:disable */
/* eslint-disable */
export function create_profile(username: string, key_info_json: string): any;
export function update_profile(profile_json: string, display_name?: string | null, bio?: string | null): any;
export function sign_profile(profile_json: string, keypair_json: string): any;
export function verify_profile(signed_profile_json: string): boolean;
export function generate_profile_magnet_uri(profile_json: string): string;
export function create_direct_message(sender_fingerprint: string, recipient_fingerprint: string, content: string): any;
export function sign_message(message_json: string, keypair_json: string): any;
export function verify_message(signed_message_json: string, public_key: string): boolean;
export function generate_keypair(): any;
export function sign_data(keypair_json: string, data: string): string;
export function verify_signature_wasm(data: string, signature: string, public_key: string): boolean;
export function storage_set_item(key: string, value: string): void;
export function storage_get_item(key: string): any;
export function storage_remove_item(key: string): void;
export function create_post(author_fingerprint: string, content: string, tags?: string[] | null, reply_to?: string | null): any;
export function sign_post(post_json: string, keypair_json: string): any;
export function verify_post(signed_post_json: string, public_key: string): boolean;
export function main(): void;
export function init_core(): void;
export class SnartNetCore {
  free(): void;
  [Symbol.dispose](): void;
  constructor();
  init(): void;
  create_profile(username: string, display_name?: string | null, bio?: string | null): string;
  get_current_profile(): any;
  update_current_profile(display_name?: string | null, bio?: string | null): void;
  create_post(content: string, tags?: string[] | null, reply_to?: string | null): any;
  create_message(recipient_fingerprint: string, content: string): any;
  get_public_key(): string;
  get_fingerprint(): string;
  has_profile(): boolean;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly create_profile: (a: number, b: number, c: number, d: number) => [number, number, number];
  readonly update_profile: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
  readonly sign_profile: (a: number, b: number, c: number, d: number) => [number, number, number];
  readonly verify_profile: (a: number, b: number) => [number, number, number];
  readonly generate_profile_magnet_uri: (a: number, b: number) => [number, number, number, number];
  readonly create_direct_message: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
  readonly sign_message: (a: number, b: number, c: number, d: number) => [number, number, number];
  readonly verify_message: (a: number, b: number, c: number, d: number) => [number, number, number];
  readonly generate_keypair: () => [number, number, number];
  readonly sign_data: (a: number, b: number, c: number, d: number) => [number, number, number, number];
  readonly verify_signature_wasm: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
  readonly __wbg_snartnetcore_free: (a: number, b: number) => void;
  readonly snartnetcore_new: () => number;
  readonly snartnetcore_init: (a: number) => [number, number];
  readonly snartnetcore_create_profile: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number, number, number];
  readonly snartnetcore_get_current_profile: (a: number) => [number, number, number];
  readonly snartnetcore_update_current_profile: (a: number, b: number, c: number, d: number, e: number) => [number, number];
  readonly snartnetcore_create_post: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number, number];
  readonly snartnetcore_create_message: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
  readonly snartnetcore_get_public_key: (a: number) => [number, number, number, number];
  readonly snartnetcore_get_fingerprint: (a: number) => [number, number, number, number];
  readonly snartnetcore_has_profile: (a: number) => number;
  readonly storage_set_item: (a: number, b: number, c: number, d: number) => [number, number];
  readonly storage_get_item: (a: number, b: number) => [number, number, number];
  readonly storage_remove_item: (a: number, b: number) => [number, number];
  readonly create_post: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number, number];
  readonly sign_post: (a: number, b: number, c: number, d: number) => [number, number, number];
  readonly verify_post: (a: number, b: number, c: number, d: number) => [number, number, number];
  readonly init_core: () => [number, number];
  readonly main: () => void;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_exn_store: (a: number) => void;
  readonly __externref_table_alloc: () => number;
  readonly __wbindgen_export_4: WebAssembly.Table;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __externref_table_dealloc: (a: number) => void;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;
/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
*
* @returns {InitOutput}
*/
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
