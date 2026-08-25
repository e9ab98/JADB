/**
 * `fastboot oem device-info` is an OEM-private command with no
 * standard output format. We treat the raw `rawLines` from Rust as
 * opaque and route through a per-OEM parser chosen by the device's
 * `product` codename.
 *
 * Adding a new vendor = add its codenames to the corresponding Set
 * below + write a parser. **No Rust or i18n changes needed** for new
 * OEMs beyond this file.
 */

/** Known Pixel codenames across Pixel 3 through Pixel 9. Source:
 *  https://en.wikipedia.org/wiki/Google_Pixel#Codenames */
const PIXEL_CODENAMES = new Set<string>([
  'blueline', 'crosshatch', // Pixel 3 / 3 XL
  'bonito', 'sargo',        // Pixel 3a / 3a XL
  'flame', 'coral',         // Pixel 4 / 4 XL
  'sunfish',                // Pixel 4a
  'bramble', 'redfin',      // Pixel 4a 5G / 5
  'barbet',                 // Pixel 5a
  'oriole', 'raven',        // Pixel 6 / 6 Pro
  'bluejay',                // Pixel 6a
  'panther', 'cheetah',     // Pixel 7 / 7 Pro
  'lynx', 'tiger',          // Pixel 7a
  'shiba', 'akita', 'husky',// Pixel 8 / 8 Pro / 8a (husky=fold)
  'komodo', 'caiman',       // Pixel 9 / 9 Pro
  'tokay',                  // Pixel 9a (guess — verify when out)
  'felix',                  // Pixel Fold (1st gen)
  'tangor', ' lynx',
]);

/** Xiaomi / Redmi / POCO codenames. Incomplete by design — we add to
 *  this set as users report models. Detection is best-effort; if we
 *  miss a codename the line still shows in the "Other fields"
 *  collapsible so they can read it. */
const XIAOMI_CODENAMES = new Set<string>([
  // Mi flagships
  'cepheus', 'perseus',         // Mi 9 / Mi Mix 3 5G
  'raphael', 'davinci',         // Mi 9T Pro / Mi 9T
  'umi', 'cmi',                 // Mi 10 / 10 Pro
  'venus',                      // Mi 11
  'zizhan', 'ditto',            // Mi 11 family extras
  'thor',                       // Mi Mix 4
  'star', 'sirius', 'mars',     // Mi 12 / 12S family
  // Redmi
  'ginkgo', 'willow',           // Redmi K20 / Note 8
  'merlin', 'lancelot',         // Redmi 9 family
  'munch', 'alioth', 'haydn',   // POCO F4 / F3 / X3 GT
  'pipa', 'veux', 'pissarro',   // Redmi K40 / K40 Pro / Note 11
  // POCO
  'beryllium',                  // POCO F1
  'lmi',                        // POCO F2 Pro
  'vayu', 'bhima',              // POCO X3 Pro / X4 Pro
  'redwood', 'ditto_poco',      // POCO X5 Pro
]);

export type OemId = 'pixel' | 'xiaomi' | 'samsung' | 'unknown';

/** Samsung `oem device-info` returns text-only diagnostic strings
 *  without the `(bootloader) Key: value` shape, so Samsung detection
 *  exists but the parser is intentionally a no-op for now. We still
 *  want to *identify* Samsung so the UI can label it correctly and
 *  show the raw lines in the collapsible without claiming parsed
 *  fields. */
const SAMSUNG_CODENAMES = new Set<string>([
  'beyond0', 'beyond1', 'beyond2', 'beyondx',     // S9 / S9+
  'd1', 'd2', 'd3',                              // Note 9
  'hero', 'hero2',                               // Note 10
  'o1', 'o2',                                    // S10
  'z3s',                                         // Fold
  'crown', 'g0', 'g1',                           // S20
  'y2s', 'z3s',                                  // various
]);

/** Pick the OEM id from the device's `product` codename. Unknown
 *  vendors return `unknown` so the UI can still show the raw lines
 *  in a collapsible. Case-insensitive — some bootloaders report
 *  `Raphael` / `RAPHAEL`. */
export function detectOem(product: string | null | undefined): OemId {
  if (!product) return 'unknown';
  const p = product.toLowerCase().trim();
  if (PIXEL_CODENAMES.has(p)) return 'pixel';
  if (XIAOMI_CODENAMES.has(p)) return 'xiaomi';
  if (SAMSUNG_CODENAMES.has(p)) return 'samsung';
  return 'unknown';
}

/**
 * Known fields that appear in (at least) Pixel's `oem device-info`
 * output. The `(bootloader) Key: value` line shape is also what
 * Xiaomi reports on most SDM bootloaders, so this parser handles
 * both. Samsung / OnePlus / others are detected but currently not
 * parsed — see `OemParsed.unrecognized`.
 */
export type ParsedOemFieldKey =
  | 'Device tampered'
  | 'Device unlocked'
  | 'Device critical unlocked'
  | 'Charger screen bypass';

export type OemParsed = {
  /** Fields we recognised and can render as badges. Missing keys
   *  stay `null` so the UI can decide whether to show "N/A" or hide
   *  the cell entirely. */
  fields: Partial<Record<ParsedOemFieldKey, string>>;
  /** Lines from the raw output that didn't match a known key.
   *  Shown verbatim in a `<details>` block so users can still read
   *  them when the parser misses a new field an OEM adds. */
  unrecognized: string[];
};

/**
 * Parse the `oem device-info` raw output. Tolerant:
 *  - Optional `(bootloader) ` prefix on each line
 *  - Trailing whitespace
 *  - Blank lines silently dropped
 *  - Lines without `Key: value` shape go to `unrecognized`
 *
 * Returns an empty `fields` object (and a copy of the input lines in
 * `unrecognized`) if the output is unparseable, so the UI can still
 * show the raw fallback.
 */
export function parseOemDeviceInfo(lines: string[]): OemParsed {
  const fields: Partial<Record<ParsedOemFieldKey, string>> = {};
  const unrecognized: string[] = [];
  for (const raw of lines) {
    const stripped = raw.replace(/^\s*\(bootloader\)\s*/, '').trim();
    if (!stripped) continue;
    const m = stripped.match(/^([^:]+?):\s*(.+?)\s*$/);
    if (!m || !m[1] || !m[2]) {
      unrecognized.push(raw);
      continue;
    }
    const key = m[1].trim() as ParsedOemFieldKey;
    const knownKeys: ParsedOemFieldKey[] = [
      'Device tampered',
      'Device unlocked',
      'Device critical unlocked',
      'Charger screen bypass',
    ];
    if ((knownKeys as string[]).includes(key)) {
      fields[key] = m[2].trim();
    } else {
      unrecognized.push(raw);
    }
  }
  return { fields, unrecognized };
}
