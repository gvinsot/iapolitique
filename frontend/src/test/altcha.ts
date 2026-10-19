async function sha256Hex(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

/**
 * Answer of GET /api/altcha/challenge for tests: a tiny proof of work (secret number 7) that
 * the real widget solves in a few milliseconds with Node's Web Crypto.
 */
export async function altchaChallenge() {
  return {
    algorithm: "SHA-256",
    challenge: await sha256Hex("sel-de-test7"),
    salt: "sel-de-test",
    signature: "signature",
    maxnumber: 50,
  };
}
