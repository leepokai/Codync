import { ApiError } from "./routes/common";

/** Enforce limits while reading, including requests without Content-Length. */
export async function readBody(req: Request, limit: number): Promise<Uint8Array> {
  if (Number(req.headers.get("content-length") ?? 0) > limit) throw new ApiError("tooLarge");
  if (!req.body) return new Uint8Array();
  const reader = req.body.getReader();
  const chunks: Uint8Array[] = [];
  let length = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      length += value.length;
      if (length > limit) {
        await reader.cancel();
        throw new ApiError("tooLarge");
      }
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }
  const result = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.length;
  }
  return result;
}
