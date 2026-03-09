// Product name generator v5: meaningful morpheme blends
// Run: bun run scripts/name-gen.ts

// Root fragments by theme (2-4 chars, each has semantic meaning)
const themeRoots: Record<string, string[]> = {
  growth: ["gro", "spr", "blo", "bud", "til", "sow", "ger", "nur",
           "cul", "vin", "fer", "rhi", "aux", "cam", "xyl", "phl",
           "lea", "sed", "ste", "har", "gle", "rea", "cro", "pru",
           "she", "mos", "lom", "bra", "ver", "flo"],
  forge:  ["for", "sme", "kil", "swa", "tem", "ing", "wel", "mol",
           "ann", "ham", "cru", "pyr"],
  graph:  ["nod", "edg", "ver", "pat", "spa", "lin", "mes", "ple",
           "dag", "tra", "gra", "cyc", "lat", "gat"],
  texture:["sol", "vel", "cor", "val", "tal", "ral", "nel", "pol",
           "vol", "fil", "mil", "kel", "del", "sel", "pal"],
};

// Product-name-quality suffixes
const suffixes = [
  // Open (vowel ending — sounds like a brand)
  "a", "i", "o", "e",
  "ra", "ri", "ro", "re",
  "na", "ni", "no", "ne",
  "la", "li", "lo", "le",
  "ta", "ti", "to",
  "ka", "ki", "ko",
  "da", "di", "do",
  "va", "vi", "vo",
  "sa", "si",
  "ma", "mi", "mo",
  "ia", "io",
  // Closed (consonant ending — sounds punchy)
  "x", "k", "n", "r", "t", "s", "d",
  "ix", "ax", "ox", "ex",
  "ik", "ak", "ok", "ek",
  "in", "an", "on", "en", "un",
  "is", "us", "os", "as",
  "al", "el", "il", "ol", "ul",
  "ar", "er", "ir", "or", "ur",
  "um", "am",
  "nt", "nd", "rn", "rk", "lt",
  // Suffix words
  "ry", "ly",
];

// Words to exclude (real words, names, products, bad connotations)
const exclude = new Set([
  // tech products
  "arc", "node", "rust", "vim", "git", "npm", "bun", "deno", "flux",
  "helm", "dart", "swift", "ruby", "perl", "java", "bash", "fish",
  "curl", "grep", "make", "rake", "gulp", "yarn", "next", "nuxt",
  "nest", "vite", "kong", "salt", "chef", "vault", "spark", "storm",
  "flink", "kafka", "redis", "flask", "rails", "gin", "echo", "iris",
  "hapi", "koa", "ray", "argo", "draft", "forge", "clerk", "neon",
  "nats", "warp", "axum", "trunk", "clap", "serde", "hyper", "tower",
  "tonic", "prost", "sprig", "vercel", "prisma", "grafana", "render",
  "linear", "notion",
  // real English words
  "grove", "graft", "bloom", "grain", "graph", "trace", "anvil",
  "smelt", "valve", "solve", "valor", "solar", "molar", "polar",
  "coral", "moral", "flora", "ultra", "extra", "spine", "spore",
  "spoke", "stole", "store", "stone", "stern", "stork", "sport",
  "verse", "verge", "nerve", "merge", "serve",
  // real names / existing brands / foreign words
  "vera", "vino", "polo", "deli", "milo", "solo", "vela", "nero",
  "nora", "tara", "sara", "lola", "mona", "nina", "dora", "rosa",
  "hugo", "palo", "mesa", "alto", "soma", "nova", "luna", "silva",
  "stella", "vita", "coda", "soda", "sonar", "delta", "sigma",
  "gamma", "alpha", "omega", "tesla", "nokia", "vesta", "flora",
  "aura", "aria", "alma", "viola", "villa", "pasta", "salsa",
  "karma", "manga", "plaza", "fiesta", "vista", "costa",
  "lino", "filo", "diva", "kilo", "halo", "vali", "mali",
  "soli", "poli", "moli", "dago", "gringo", "lira", "soma",
  "gala", "cola", "coma", "sofa", "camo", "demo",
  "venom", "valor", "vigor", "manor", "minor", "major",
  "solar", "lunar", "molar", "tidal", "modal", "nodal", "tonal",
  "coral", "moral", "viral", "rival", "total",
  "sever", "lever", "fever", "river", "liver", "diver", "giver",
  "paver", "raver", "saver", "waver",
  "verdi", "verde", "verso", "grotto", "gelato",
  "ferro", "terra", "serra", "guerra",
  "nori", "saki", "sake", "tofu", "ramen",
  // profanity / bad connotations in any language
  "ass", "cum", "fag", "gag", "hag", "hex", "hoe", "nag", "pee",
  "pig", "poo", "pox", "pus", "rat", "rot", "rut", "sag", "sin",
  "sob", "sod", "sot", "sty", "tit", "vex", "woe", "damn", "dumb",
  "dump", "fart", "grim", "harm", "hate", "hell", "hurt", "jerk",
  "lame", "mess", "mock", "muck", "null", "punk", "rash", "scam",
  "slag", "slap", "slob", "slug", "slum", "slur", "smut", "snot",
  "spam", "spit", "thud", "turd", "wart", "wimp", "crap", "crud",
  "dork", "dung", "bile", "barf", "gunk", "lewd", "con", "cul",
  "pute", "culo", "puta", "mala", "malo", "kot", "kuso", "pau",
  "merda", "shat", "arse", "fick", "cunt", "shit", "piss",
  "grope", "groan", "groin",
]);

function isPronounceable(word: string): boolean {
  if (/[^aeiou]{4,}/.test(word)) return false;
  if (/[aeiou]{3,}/.test(word)) return false;
  if (!/[aeiou]/.test(word)) return false;
  if (/^[^aeiou]{3,}/.test(word) && !/^(str|spr|scr|spl|thr)/.test(word)) return false;
  // No weird consonant clusters mid-word
  if (/[^aeiou]{3}/.test(word.slice(1, -1))) return false;
  return true;
}

function generate(): Set<string> {
  const words = new Set<string>();
  const allRoots = Object.values(themeRoots).flat();

  // Strategy 1: root + suffix
  for (const root of allRoots) {
    for (const suf of suffixes) {
      const w = root + suf;
      if (w.length >= 3 && w.length <= 6 && isPronounceable(w)) {
        words.add(w);
      }
    }
  }

  // Strategy 2: root + root (short roots only)
  const short = allRoots.filter(r => r.length <= 3);
  for (const r1 of short) {
    for (const r2 of short) {
      if (r1 === r2) continue;
      const w = r1 + r2;
      if (w.length >= 4 && w.length <= 6 && isPronounceable(w)) {
        words.add(w);
      }
    }
  }

  // Strategy 3: blended overlaps (share a letter)
  for (const r1 of allRoots) {
    for (const r2 of allRoots) {
      if (r1 === r2) continue;
      const last = r1[r1.length - 1];
      const first = r2[0];
      if (last === first) {
        const w = r1 + r2.slice(1);
        if (w.length >= 4 && w.length <= 6 && isPronounceable(w)) {
          words.add(w);
        }
      }
    }
  }

  // Strategy 4: root with internal vowel swap/addition
  for (const root of allRoots) {
    for (const v of ["a", "e", "i", "o", "u"]) {
      // Insert vowel after first consonant cluster
      const match = root.match(/^([^aeiou]+)(.*)/);
      if (match) {
        const w = match[1] + v + match[2];
        if (w.length >= 3 && w.length <= 6 && isPronounceable(w)) {
          words.add(w);
        }
      }
      // Append vowel
      const w2 = root + v;
      if (w2.length >= 3 && w2.length <= 6 && isPronounceable(w2)) {
        words.add(w2);
      }
    }
  }

  return words;
}

function score(word: string): number {
  if (exclude.has(word)) return -100;

  let s = 0;

  // === Syllable structure (most important) ===
  const isCVCV = /^[^aeiou]{1,3}[aeiou][^aeiou][aeiou]$/.test(word);
  const isCVCCV = /^[^aeiou]{1,2}[aeiou][^aeiou]{2}[aeiou]$/.test(word);
  const isCVCVC = /^[^aeiou]{1,2}[aeiou][^aeiou][aeiou][^aeiou]$/.test(word);
  const isCVC = /^[^aeiou]{1,3}[aeiou]{1,2}[^aeiou]{1,2}$/.test(word);

  if (isCVCV) s += 12;
  else if (isCVCCV) s += 10;
  else if (isCVCVC) s += 10;
  else if (isCVC) s += 5;
  else s -= 3;

  // === Length ===
  if (word.length === 5) s += 8;
  if (word.length === 4) s += 6;
  if (word.length === 6) s += 3;
  if (word.length === 3) s += 1;

  // === Ending quality ===
  if (/[aio]$/.test(word)) s += 4;
  if (/[xk]$/.test(word)) s += 4;
  if (/e$/.test(word)) s += 2;
  if (/[rln]$/.test(word)) s += 1;

  // === Starting distinctiveness ===
  if (/^(gr|tr|cr|br|sp|fl|gl|pr|dr|sk|sw|th)/.test(word)) s += 3;
  if (/^(v|z|j|qu|kn|wr)/.test(word)) s += 4;

  // === Uniqueness ===
  if (/(.)\1/.test(word)) s -= 6;
  // Penalize if it exactly matches a common 3-letter word at start
  const common3 = ["the", "and", "for", "are", "but", "not", "all", "can",
    "had", "was", "has", "his", "how", "its", "let", "may", "new",
    "now", "old", "see", "way", "who", "run", "say", "too", "use"];
  for (const c of common3) {
    if (word.startsWith(c) && word.length <= 5) s -= 3;
  }

  return s;
}

function main() {
  const candidates = generate();
  const scored = [...candidates]
    .filter(w => !exclude.has(w))
    .map(w => ({ word: w, score: score(w) }))
    .filter(w => w.score >= 18)
    .sort((a, b) => b.score - a.score);

  console.log(`Generated ${candidates.size} candidates, ${scored.length} good ones\n`);

  // Dedup: max 2 per 3-char prefix
  const prefixCount = new Map<string, number>();
  const diverse: typeof scored = [];
  for (const item of scored) {
    const prefix = item.word.slice(0, 3);
    const count = prefixCount.get(prefix) ?? 0;
    if (count < 2) {
      diverse.push(item);
      prefixCount.set(prefix, count + 1);
    }
  }

  console.log(`${diverse.length} after dedup:\n`);
  for (let i = 0; i < Math.min(diverse.length, 250); i++) {
    const { word, score } = diverse[i];
    process.stdout.write(`  ${word.padEnd(9)}`);
    if ((i + 1) % 7 === 0) process.stdout.write("\n");
  }
  console.log();
}

main();
