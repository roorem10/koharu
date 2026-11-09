// Rough Rust delimiter checker: reports the first mismatch, skipping strings, chars, comments.
const fs = require('fs')
for (const f of process.argv.slice(2)) {
  const s = fs.readFileSync(f, 'utf8')
  const st = []
  let i = 0, line = 1, bad = null
  const pair = { ')': '(', ']': '[', '}': '{' }
  while (i < s.length && !bad) {
    const c = s[i]
    if (c === '\n') { line++; i++; continue }
    if (s.startsWith('//', i)) { while (i < s.length && s[i] !== '\n') i++; continue }
    if (s.startsWith('/*', i)) { const e = s.indexOf('*/', i); line += s.slice(i, e).split('\n').length - 1; i = e + 2; continue }
    const raw = /^b?r(#*)"/.exec(s.slice(i, i + 12))
    if (raw && !/\w/.test(s[i - 1] || '')) {
      const end = '"' + raw[1]
      const e = s.indexOf(end, i + raw[0].length)
      line += s.slice(i, e).split('\n').length - 1
      i = e + end.length
      continue
    }
    if (c === '"') {
      i++
      while (s[i] !== '"') { if (s[i] === '\\') i++; if (s[i] === '\n') line++; i++ }
      i++
      continue
    }
    if (c === "'") {
      const m = /^'(\\.[^']*|[^\\'])'/.exec(s.slice(i, i + 14))
      if (m) { i += m[0].length; continue }
      i++
      continue
    }
    if ('([{'.includes(c)) st.push([c, line])
    else if (')]}'.includes(c)) {
      const t = st.pop()
      if (!t || t[0] !== pair[c]) bad = `line ${line}: ${c} closes ${t ? t[0] + ' from line ' + t[1] : 'nothing'}`
    }
    i++
  }
  console.log(f, bad || (st.length ? `unclosed ${JSON.stringify(st.slice(-3))}` : 'ok'))
}
