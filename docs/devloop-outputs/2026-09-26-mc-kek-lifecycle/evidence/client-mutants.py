import sys
src=open('/tmp/client-evidence/receivePath.fixed.ts').read()
m=sys.argv[1]
def rep(old,new):
    global src
    assert src.count(old)==1,(m,old[:60])
    src=src.replace(old,new)
SET_HEAD="""    const cacheKey = keyIdCacheKey(keyId);
    const existing = forSender.get(cacheKey);
"""
if m=='scoped_overwrite':
    # Pre-E-1: re-wrap under another generation is unwrapped and OVERWRITES,
    # dragging the key id's scope to the new generation.
    rep("    if (scope !== undefined && wrapped.kekGeneration !== scope) {","    if (false) {")
    rep("""      if (existing.generation !== generation) {
        key.fill(0);
        return false;
      }""","""      if (existing.generation !== generation) {
        forSender.set(cacheKey, { key, wrap: Uint8Array.from(wrap), generation });
        this.#bySender.set(sender, forSender);
        return true;
      }""")
    rep("  if (fresh && installedMeanwhile !== undefined && installedMeanwhile !== fresh.generation) {","  if (false) {")
elif m=='m1':
    rep(SET_HEAD, SET_HEAD+"""    for (const other of this.#bySender.values())
      for (const e of other.values()) if (generation > e.generation) { key.fill(0); return false; }
""")
elif m=='m2':
    rep(SET_HEAD, SET_HEAD+"""    for (const e of forSender.values()) if (generation > e.generation) { key.fill(0); return false; }
""")
open('src/media/frame/receivePath.ts','w').write(src)
