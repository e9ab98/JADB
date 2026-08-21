import type { ApkInfo } from '@/ipc/analyze';
import type { RuleReport } from '@/ipc/rules';

/**
 * Self-contained HTML compare report. No external assets — opens
 * cleanly in any browser, prints/archives as a single `.html`.
 *
 * Mirrors the styling of the existing single-APK report template
 * (see `src/features/apkAnalyze/reportTemplate.ts`) but extends it
 * with side-by-side columns and "added / removed / shared" diffs
 * for permissions, components, native libs, and rule hits.
 */

type SideEntry = { info: ApkInfo; rules: RuleReport | null };

export type ComparePayload = {
  a: SideEntry;
  b: SideEntry;
};

function escape(s: string | null | undefined): string {
  if (s == null) return '—';
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function formatSize(bytes: number | null | undefined): string {
  if (!bytes) return '—';
  const k = 1024;
  const units = ['B', 'KB', 'MB', 'GB'];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), units.length - 1);
  const v = bytes / Math.pow(k, i);
  return `${v.toFixed(2)} ${units[i]}`;
}

function sdkSummary(info: ApkInfo): string {
  const parts: string[] = [];
  if (info.min_sdk) parts.push(`min ${escape(info.min_sdk)}`);
  if (info.target_sdk) parts.push(`target ${escape(info.target_sdk)}`);
  if (info.max_sdk) parts.push(`max ${escape(info.max_sdk)}`);
  return parts.length > 0 ? parts.join(' / ') : '—';
}

function diffSets(a: Iterable<string>, b: Iterable<string>) {
  const aSet = new Set(a);
  const bSet = new Set(b);
  const added: string[] = [];
  const removed: string[] = [];
  const shared: string[] = [];
  for (const item of aSet) {
    if (bSet.has(item)) shared.push(item);
    else removed.push(item);
  }
  for (const item of bSet) {
    if (!aSet.has(item)) added.push(item);
  }
  added.sort();
  removed.sort();
  shared.sort();
  return { added, removed, shared };
}

function diffRules(a: RuleReport | null, b: RuleReport | null) {
  type Row = { label: string; severity: string };
  const collect = (r: RuleReport | null): Map<string, Row> => {
    const out = new Map<string, Row>();
    if (!r) return out;
    for (const cat of Object.values(r.components)) {
      for (const hit of cat) {
        if (!hit.matched_rule) continue;
        const label = hit.matched_rule.metadata?.label?.trim() || hit.name;
        out.set(label, { label, severity: (hit.matched_rule.severity ?? 'info').toLowerCase() });
      }
    }
    return out;
  };
  const mapA = collect(a);
  const mapB = collect(b);
  const added: Row[] = [];
  const removed: Row[] = [];
  const shared: Row[] = [];
  for (const [k, v] of mapA) {
    if (mapB.has(k)) shared.push(v);
    else removed.push(v);
  }
  for (const [k, v] of mapB) {
    if (!mapA.has(k)) added.push(v);
  }
  added.sort((x, y) => x.label.localeCompare(y.label));
  removed.sort((x, y) => x.label.localeCompare(y.label));
  shared.sort((x, y) => x.label.localeCompare(y.label));
  return { added, removed, shared };
}

function diffColumnHtml(title: string, items: string[], tone: 'add' | 'remove' | 'shared'): string {
  const cls = {
    add: 'col-add',
    remove: 'col-remove',
    shared: 'col-shared',
  }[tone];
  const list = items.length === 0
    ? '<div class="empty">—</div>'
    : `<ul class="items">${items.map((it) => `<li>${escape(it)}</li>`).join('')}</ul>`;
  return `<div class="diff-col ${cls}">
    <div class="diff-head">${escape(title)} <span class="count">${items.length}</span></div>
    ${list}
  </div>`;
}

function diffRulesHtml(diff: { added: { label: string; severity: string }[]; removed: { label: string; severity: string }[]; shared: { label: string; severity: string }[] }): string {
  const renderColumn = (
    title: string,
    items: { label: string; severity: string }[],
    tone: 'add' | 'remove' | 'shared',
  ) => {
    const cls = { add: 'col-add', remove: 'col-remove', shared: 'col-shared' }[tone];
    const list = items.length === 0
      ? '<div class="empty">—</div>'
      : `<ul class="items">${items
          .map(
            (it) =>
              `<li><span class="rule-label">${escape(it.label)}</span><span class="rule-sev sev-${escape(it.severity)}">${escape(it.severity)}</span></li>`,
          )
          .join('')}</ul>`;
    return `<div class="diff-col ${cls}">
      <div class="diff-head">${escape(title)} <span class="count">${items.length}</span></div>
      ${list}
    </div>`;
  };
  return `
    <div class="diff-grid">
      ${renderColumn('Added in B', diff.added, 'add')}
      ${renderColumn('Removed from A', diff.removed, 'remove')}
      ${renderColumn('Shared', diff.shared, 'shared')}
    </div>`;
}

export function buildCompareReportHtml(payload: ComparePayload): string {
  const a = payload.a.info;
  const b = payload.b.info;

  const perms = diffSets(a.permissions, b.permissions);
  const activities = diffSets(a.activities, b.activities);
  const services = diffSets(a.services, b.services);
  const receivers = diffSets(a.receivers, b.receivers);
  const providers = diffSets(a.providers, b.providers);
  const nativeLibs = diffSets(a.native_libs ?? [], b.native_libs ?? []);
  const rules = diffRules(payload.a.rules, payload.b.rules);

  const metaRow = (label: string, left: string, right: string, changed: boolean) => {
    const cls = changed ? 'cell changed' : 'cell';
    return `<div class="meta-row"><div class="meta-label">${escape(label)}</div><div class="${cls}">${escape(left)}</div><div class="${cls}">${escape(right)}</div></div>`;
  };

  const metaRows = [
    metaRow('Package', a.package_name, b.package_name, a.package_name !== b.package_name),
    metaRow('Label', a.application_label ?? '', b.application_label ?? '', a.application_label !== b.application_label),
    metaRow('Version name', a.version_name ?? '', b.version_name ?? '', a.version_name !== b.version_name),
    metaRow('Version code', a.version_code ?? '', b.version_code ?? '', a.version_code !== b.version_code),
    metaRow('SDK', sdkSummary(a), sdkSummary(b), sdkSummary(a) !== sdkSummary(b)),
    metaRow('File size', formatSize(a.file_size), formatSize(b.file_size), a.file_size !== b.file_size),
  ].join('');

  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8" />
<title>JADB APK Compare — ${escape(a.package_name)} vs ${escape(b.package_name)}</title>
<style>
  :root {
    --bg: #f8fafc;
    --surface: #ffffff;
    --border: #e2e8f0;
    --text: #0f172a;
    --text-2: #64748b;
    --brand: #2563eb;
    --success: #16a34a;
    --warning: #d97706;
    --danger: #dc2626;
  }
  * { box-sizing: border-box; }
  body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; margin: 0; padding: 32px; background: var(--bg); color: var(--text); }
  h1 { margin: 0 0 8px; font-size: 24px; }
  .sub { color: var(--text-2); margin-bottom: 24px; font-size: 13px; }
  .panel { background: var(--surface); border: 1px solid var(--border); border-radius: 12px; padding: 20px; margin-bottom: 16px; }
  .panel-title { display: flex; align-items: center; gap: 8px; font-weight: 600; margin-bottom: 12px; font-size: 14px; }
  .panel-title::before { content: ''; width: 8px; height: 8px; border-radius: 4px; background: var(--brand); }
  .meta-grid { display: grid; grid-template-columns: 140px 1fr 1fr; gap: 8px; font-size: 13px; }
  .meta-row { display: contents; }
  .meta-label { color: var(--text-2); padding: 4px 0; }
  .cell { padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; font-family: ui-monospace, Menlo, monospace; font-size: 12px; background: #f8fafc; }
  .cell.changed { border-color: var(--warning); background: #fef3c7; color: var(--warning); }
  .diff-grid { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 12px; }
  .diff-col { border-radius: 8px; padding: 12px; }
  .col-add { background: #ecfdf5; border: 1px solid #a7f3d0; }
  .col-remove { background: #fef2f2; border: 1px solid #fecaca; }
  .col-shared { background: #f8fafc; border: 1px solid var(--border); }
  .diff-head { font-weight: 600; font-size: 12px; margin-bottom: 6px; display: flex; justify-content: space-between; }
  .count { color: var(--text-2); font-weight: 400; }
  .empty { color: var(--text-2); font-size: 11px; }
  ul.items { list-style: none; margin: 0; padding: 0; max-height: 240px; overflow-y: auto; }
  ul.items li { padding: 3px 6px; font-family: ui-monospace, Menlo, monospace; font-size: 11px; border-bottom: 1px solid rgba(15, 23, 42, 0.05); }
  ul.items li:last-child { border-bottom: 0; }
  .rule-label { flex: 1; }
  .rule-sev { padding: 1px 6px; border-radius: 4px; font-size: 10px; text-transform: uppercase; font-weight: 700; margin-left: 6px; }
  .sev-info { background: #e0e7ff; color: #3730a3; }
  .sev-warn, .sev-warning { background: #fef3c7; color: #92400e; }
  .sev-danger { background: #fee2e2; color: #991b1b; }
  .footer { margin-top: 32px; color: var(--text-2); font-size: 11px; text-align: center; }
  @media print { body { padding: 12px; } .panel { page-break-inside: avoid; } }
</style>
</head>
<body>
  <h1>JADB APK Compare</h1>
  <p class="sub">${escape(a.package_name)} (${escape(a.version_name ?? '?')}) vs ${escape(b.package_name)} (${escape(b.version_name ?? '?')})</p>

  <div class="panel">
    <div class="panel-title">Metadata</div>
    <div class="meta-grid">
      <div></div>
      <div style="font-weight:600;color:var(--text-2);font-size:11px;text-transform:uppercase;">A</div>
      <div style="font-weight:600;color:var(--text-2);font-size:11px;text-transform:uppercase;">B</div>
      ${metaRows}
    </div>
  </div>

  <div class="panel">
    <div class="panel-title">Permissions</div>
    <div class="diff-grid">
      ${diffColumnHtml('Added in B', perms.added, 'add')}
      ${diffColumnHtml('Removed from A', perms.removed, 'remove')}
      ${diffColumnHtml('Shared', perms.shared, 'shared')}
    </div>
  </div>

  <div class="panel">
    <div class="panel-title">Components</div>
    <h3 style="font-size:13px;margin:16px 0 8px;">Activities</h3>
    <div class="diff-grid">
      ${diffColumnHtml('Added in B', activities.added, 'add')}
      ${diffColumnHtml('Removed from A', activities.removed, 'remove')}
      ${diffColumnHtml('Shared', activities.shared, 'shared')}
    </div>
    <h3 style="font-size:13px;margin:16px 0 8px;">Services</h3>
    <div class="diff-grid">
      ${diffColumnHtml('Added in B', services.added, 'add')}
      ${diffColumnHtml('Removed from A', services.removed, 'remove')}
      ${diffColumnHtml('Shared', services.shared, 'shared')}
    </div>
    <h3 style="font-size:13px;margin:16px 0 8px;">Receivers</h3>
    <div class="diff-grid">
      ${diffColumnHtml('Added in B', receivers.added, 'add')}
      ${diffColumnHtml('Removed from A', receivers.removed, 'remove')}
      ${diffColumnHtml('Shared', receivers.shared, 'shared')}
    </div>
    <h3 style="font-size:13px;margin:16px 0 8px;">Providers</h3>
    <div class="diff-grid">
      ${diffColumnHtml('Added in B', providers.added, 'add')}
      ${diffColumnHtml('Removed from A', providers.removed, 'remove')}
      ${diffColumnHtml('Shared', providers.shared, 'shared')}
    </div>
  </div>

  <div class="panel">
    <div class="panel-title">Native Libraries</div>
    <div class="diff-grid">
      ${diffColumnHtml('Added in B', nativeLibs.added, 'add')}
      ${diffColumnHtml('Removed from A', nativeLibs.removed, 'remove')}
      ${diffColumnHtml('Shared', nativeLibs.shared, 'shared')}
    </div>
  </div>

  <div class="panel">
    <div class="panel-title">Rule Hits</div>
    ${diffRulesHtml(rules)}
  </div>

  <div class="footer">Generated by JADB at ${new Date().toISOString()}</div>
</body>
</html>`;
}
