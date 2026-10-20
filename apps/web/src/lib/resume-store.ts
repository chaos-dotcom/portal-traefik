// A tiny IndexedDB key-value store for resumable uploads. localStorage is too small: a big file's
// presigned chunk URLs alone can run to megabytes. Every call fails soft (private mode, blocked
// storage): resuming is a convenience, never a requirement.

const DB_NAME = "portal-uploads";
const STORE = "kv";
let dbp: Promise<IDBDatabase> | null = null;

function db(): Promise<IDBDatabase> {
  if (!dbp) {
    dbp = new Promise((resolve, reject) => {
      const req = indexedDB.open(DB_NAME, 1);
      req.onupgradeneeded = () => req.result.createObjectStore(STORE);
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
    dbp.catch(() => (dbp = null));
  }
  return dbp;
}

async function tx<T>(mode: IDBTransactionMode, fn: (s: IDBObjectStore) => IDBRequest<T>): Promise<T | undefined> {
  try {
    const d = await db();
    return await new Promise<T>((resolve, reject) => {
      const req = fn(d.transaction(STORE, mode).objectStore(STORE));
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
  } catch {
    return undefined;
  }
}

export const kvGet = <T>(key: string) => tx<T>("readonly", (s) => s.get(key) as IDBRequest<T>);
export const kvSet = (key: string, value: unknown) => tx("readwrite", (s) => s.put(value, key));
export const kvDel = (key: string) => tx("readwrite", (s) => s.delete(key));

/** Every key starting with `prefix`. */
export async function kvKeys(prefix: string): Promise<string[]> {
  const keys = (await tx<IDBValidKey[]>("readonly", (s) => s.getAllKeys())) ?? [];
  return keys.map(String).filter((k) => k.startsWith(prefix));
}
