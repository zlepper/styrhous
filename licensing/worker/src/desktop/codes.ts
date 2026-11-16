// RFC 8628 §6.1: an uppercase consonant alphabet avoids ambiguous characters and words.
const userCodeAlphabet = 'BCDFGHJKLMNPQRSTVWXZ';

/** An 8-character user code formatted as `XXXX-XXXX` (about 34 bits of entropy). */
export function generateUserCode(): string {
  const limit = 256 - (256 % userCodeAlphabet.length);
  const characters: string[] = [];
  while (characters.length < 8) {
    for (const byte of crypto.getRandomValues(new Uint8Array(16))) {
      if (byte < limit && characters.length < 8) {
        characters.push(userCodeAlphabet[byte % userCodeAlphabet.length]!);
      }
    }
  }
  return `${characters.slice(0, 4).join('')}-${characters.slice(4).join('')}`;
}

/** Accepts user codes typed in any case, with or without the separator. */
export function normalizeUserCode(value: string): string {
  const compact = value.toUpperCase().replace(/[^A-Z]/g, '');
  return compact.length === 8 ? `${compact.slice(0, 4)}-${compact.slice(4)}` : compact;
}
