// Gives a worktree its own node_modules made of junctions into the main checkout's,
// except @koharu/* which point at the worktree's packages, so tests see its bindings.
const fs = require('fs'), path = require('path')
const [repo, wt] = process.argv.slice(2)
const src = path.join(repo, 'node_modules'), dst = path.join(wt, 'node_modules')
fs.mkdirSync(dst, { recursive: true })
for (const e of fs.readdirSync(src, { withFileTypes: true })) {
  if (['.bin', '.vite', '.cache', '.vite-temp'].includes(e.name)) continue
  if (e.name === '@koharu') {
    const d = path.join(dst, e.name)
    fs.mkdirSync(d)
    for (const [n, p] of [['app', 'koharu'], ['bridge', 'bridge'], ['docs', 'docs'], ['ui', 'ui']])
      fs.symlinkSync(path.join(wt, 'packages', p), path.join(d, n), 'junction')
    continue
  }
  fs.symlinkSync(path.join(src, e.name), path.join(dst, e.name), 'junction')
}
