// usage: node res.js spec.json  where spec = { "file": ["both"|"ours"|"theirs"|"custom text", ...] } per conflict hunk in order
const fs = require('fs')
const spec = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'))
for (const [f, picks] of Object.entries(spec)) {
  const crlf = fs.readFileSync(f, 'utf8').includes('\r\n')
  const lines = fs.readFileSync(f, 'utf8').replace(/\r\n/g, '\n').split('\n')
  const out = []
  let i = 0, n = 0
  while (i < lines.length) {
    if (!lines[i].startsWith('<<<<<<< ')) { out.push(lines[i++]); continue }
    const ours = [], theirs = []
    i++
    while (!lines[i].startsWith('=======')) { if (lines[i].startsWith('||||||| ')) { i++; while (!lines[i].startsWith('=======')) i++; break } ours.push(lines[i++]) }
    i++
    while (!lines[i].startsWith('>>>>>>> ')) theirs.push(lines[i++])
    i++
    const p = picks[n++]
    if (p === undefined) throw new Error(`${f}: no pick for hunk ${n}`)
    if (p === 'both') out.push(...ours, ...theirs)
    else if (p === 'bothr') out.push(...theirs, ...ours)
    else if (p === 'bothtest') out.push(...ours, '  })', '', ...theirs)
    else if (p === 'ours') out.push(...ours)
    else if (p === 'theirs') out.push(...theirs)
    else out.push(...p.split('\n'))
  }
  if (n !== picks.length) throw new Error(`${f}: ${n} hunks but ${picks.length} picks`)
  fs.writeFileSync(f, out.join('\n'))
  void crlf
}
