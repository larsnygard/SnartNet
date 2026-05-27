declare module '../wasm/snartnet_core.js' {
  export function initWasm(): Promise<void>;
  export function init_core(): void;
  export class SnartNetCore {}
  export function generate_keypair(): Uint8Array;
  export function sign_data(data: Uint8Array): Uint8Array;
  export function verify_signature_wasm(...args: any[]): boolean;
}

