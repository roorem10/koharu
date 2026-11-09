// Merges the WO branches in order into the current worktree's branch. Locale files are
// deep-merged and purely additive files unioned; stops at the first conflict needing a person.
const { execSync, spawnSync } = require('child_process')
const fs = require('fs')
const sh = (c, o = {}) => execSync(c, { encoding: 'utf8', maxBuffer: 1 << 28, stdio: ['ignore', 'pipe', 'pipe'], ...o })
const id = '-c user.name=roorem10 -c user.email=roorem10@users.noreply.github.com'
const UNION = [/packages\/bridge\/src\/protocol\.ts$/, /tests\/setup\.ts$/, /commands\/mod\.rs$/, /agent\/host\.rs$/, /mcp\/server\.rs$/]
const show = (stage, f) => { try { return sh(`git show :${stage}:"${f}"`) } catch { return null } }
const norm = (s) => s && s.replace(/\r\n/g, '\n')
function deep(base, a, b) {
  if (typeof a !== 'object' || a === null || typeof b !== 'object' || b === null) {
    return JSON.stringify(a) === JSON.stringify(base) ? b : a
  }
  const out = { ...a }
  for (const k of Object.keys(b)) {
    if (!(k in a)) { if (!(base && k in base)) out[k] = b[k]; else if (JSON.stringify(base[k]) !== JSON.stringify(b[k])) out[k] = b[k]; continue }
    out[k] = deep(base ? base[k] : undefined, a[k], b[k])
  }
  return out
}
const branches = process.argv.slice(2)
const done = sh('git log --merges --format=%s').split('\n')
for (const br of branches) {
  if (done.some((s) => s.includes(`'${br}'`))) continue
  const r = spawnSync('git', [...id.split(' '), 'merge', '--no-ff', '--no-commit', br], { encoding: 'utf8' })
  const conflicted = sh('git diff --name-only --diff-filter=U').split('\n').filter(Boolean)
  const left = []
  for (const f of conflicted) {
    if (/locales\/.*translation\.json$/.test(f)) {
      const [o, a, b] = [1, 2, 3].map((s) => JSON.parse(norm(show(s, f))))
      fs.writeFileSync(f, JSON.stringify(deep(o, a, b), null, 2) + '\n')
      sh(`git add "${f}"`)
    } else if (UNION.some((p) => p.test(f)) && show(1, f) !== null) {
      const tmp = [1, 2, 3].map((s) => { const p = `.git-tmp-${s}`; fs.writeFileSync(p, norm(show(s, f))); return p })
      spawnSync('git', ['merge-file', '--union', '-p', tmp[1], tmp[0], tmp[2]], { encoding: 'utf8', maxBuffer: 1 << 28 })
      const res = spawnSync('git', ['merge-file', '--union', '-p', tmp[1], tmp[0], tmp[2]], { encoding: 'utf8', maxBuffer: 1 << 28 }).stdout
      fs.writeFileSync(f, res)
      tmp.forEach((p) => fs.unlinkSync(p))
      sh(`git add "${f}"`)
    } else left.push(f)
  }
  if (left.length) {
    console.log(`STOP at ${br}: needs hand resolution:\n` + left.join('\n'))
    process.exit(2)
  }
  sh(`git ${id} commit -q --no-edit`)
  console.log(`merged ${br} (auto-resolved ${conflicted.length})`)
}
console.log('ALL MERGED')
