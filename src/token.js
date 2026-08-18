import { timingSafeEqual, randomBytes } from "node:crypto";
import { mkdir, open, readFile } from "node:fs/promises";
import { dirname } from "node:path";

export function createToken() {
  return randomBytes(32).toString("base64url");
}

export async function loadOrCreateToken(tokenPath) {
  try {
    return (await readFile(tokenPath, "utf8")).trim();
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }

  await mkdir(dirname(tokenPath), { recursive: true, mode: 0o700 });
  const token = createToken();

  try {
    const handle = await open(tokenPath, "wx", 0o600);
    try {
      await handle.writeFile(`${token}\n`, "utf8");
    } finally {
      await handle.close();
    }
    return token;
  } catch (error) {
    if (error?.code !== "EEXIST") throw error;
    return (await readFile(tokenPath, "utf8")).trim();
  }
}

export function tokensEqual(actual, expected) {
  if (typeof actual !== "string" || typeof expected !== "string") return false;
  const left = Buffer.from(actual);
  const right = Buffer.from(expected);
  return left.length === right.length && timingSafeEqual(left, right);
}
