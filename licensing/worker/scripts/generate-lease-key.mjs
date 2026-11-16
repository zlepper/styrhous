// Prints a new Ed25519 lease signing key. Store `privateJwk` as the LEASE_SIGNING_KEY secret;
// after rotating, append the old `publicJwk` to LEASE_PREVIOUS_PUBLIC_KEYS.
import { generateKeyPairSync } from 'node:crypto';

const { privateKey, publicKey } = generateKeyPairSync('ed25519');
console.log(
  JSON.stringify(
    {
      privateJwk: privateKey.export({ format: 'jwk' }),
      publicJwk: publicKey.export({ format: 'jwk' })
    },
    null,
    2
  )
);
