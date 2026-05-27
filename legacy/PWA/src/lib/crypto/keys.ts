export async function generateKeyPair() {
  return await window.crypto.subtle.generateKey(
    {
      name: "Ed25519",
      namedCurve: "Ed25519"
    },
    true,
    ["sign", "verify"]
  );
}
