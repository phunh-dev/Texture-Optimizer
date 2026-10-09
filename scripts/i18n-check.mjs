// Verifies every locale has the same namespace files and the same keys as
// the reference locale (en). Exits with code 1 on any mismatch.
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

const root = join(import.meta.dirname, '..', 'src', 'locales')
const reference = 'en'

function flatten(obj, prefix = '') {
  return Object.entries(obj).flatMap(([k, v]) =>
    v !== null && typeof v === 'object' ? flatten(v, `${prefix}${k}.`) : [[`${prefix}${k}`, v]],
  )
}

function load(lang) {
  const files = readdirSync(join(root, lang)).filter((f) => f.endsWith('.json'))
  return Object.fromEntries(
    files.map((f) => [f, new Map(flatten(JSON.parse(readFileSync(join(root, lang, f), 'utf8'))))]),
  )
}

const langs = readdirSync(root, { withFileTypes: true }).filter((d) => d.isDirectory()).map((d) => d.name)
const ref = load(reference)
const problems = []

for (const lang of langs.filter((l) => l !== reference)) {
  const other = load(lang)
  for (const file of new Set([...Object.keys(ref), ...Object.keys(other)])) {
    if (!ref[file]) { problems.push(`${lang}/${file}: file missing in ${reference}`); continue }
    if (!other[file]) { problems.push(`${lang}/${file}: file missing`); continue }
    for (const key of ref[file].keys()) {
      if (!other[file].has(key)) problems.push(`${lang}/${file}: missing key "${key}"`)
      else if (other[file].get(key) === '') problems.push(`${lang}/${file}: empty value for "${key}"`)
    }
    for (const key of other[file].keys()) {
      if (!ref[file].has(key)) problems.push(`${lang}/${file}: extra key "${key}" (not in ${reference})`)
    }
  }
}

if (problems.length) {
  console.error(`i18n check failed (${problems.length} problem(s)):`)
  for (const p of problems) console.error(`  - ${p}`)
  process.exit(1)
}
console.log(`i18n check passed: ${langs.join(', ')} are in sync.`)
