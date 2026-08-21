import { useEffect, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
import { toast } from 'sonner';
import {
  AlertTriangle,
  ArrowUp,
  Download,
  Eraser,
  File as FileIcon,
  Folder as FolderIcon,
  FolderOpen,
  Link as LinkIcon,
  Loader2,
  RefreshCw,
  Upload,
} from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  deleteRemoteFile,
  listRemoteDir,
  pullFile,
  pushFile,
  type DirEntry,
} from '@/ipc/adb';
import { cn } from '@/lib/utils';

/**
 * File-manager window rendered for `/data-dir?device=…&pkg=…&as=…`. The
 * `as` query param carries the package name when the listing should run
 * via `run-as <pkg>` (debuggable app); `root=1` selects `su` for rooted
 * release apps. Both modes are determined up-front by the AppCard probe.
 */
export function DataDirView() {
  const { t } = useTranslation();
  const [searchParams] = useSearchParams();
  const device = searchParams.get('device');
  const pkg = searchParams.get('pkg');
  const resolvedRootPath = searchParams.get('path');
  // `as` carries the package for debug apps; `root=1` elevates release apps.
  const asPkg = searchParams.get('as');
  const useRoot = searchParams.get('root') === '1';
  const rootPath =
    resolvedRootPath ?? (device && pkg ? `/data/data/${pkg}` : '/data/data');
  const [currentPath, setCurrentPath] = useState<string>(rootPath);
  const [entries, setEntries] = useState<DirEntry[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<DirEntry | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [pushing, setPushing] = useState(false);
  const [downloadingPath, setDownloadingPath] = useState<string | null>(null);

  async function refresh(path: string = currentPath) {
    setLoading(true);
    setError(null);
    try {
      const list = await listRemoteDir(safeDevice, path, asPkg, useRoot);
      setEntries(list);
      setCurrentPath(path);
    } catch (e) {
      const msg = String(e);
      setError(msg);
      setEntries(null);
    } finally {
      setLoading(false);
    }
  }

  // Initial load when device / pkg / asPkg changes.
  useEffect(() => {
    if (device && pkg) {
      void refresh(rootPath);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [device, pkg, asPkg, useRoot, rootPath]);

  if (!device || !pkg) {
    return (
      <CenteredMessage
        title={t('dataDir.missingParams')}
        description={t('dataDir.missingParamsDesc')}
      />
    );
  }
  // Narrowed locals — closures defined below don't inherit the narrowing
  // from the early return above.
  const safeDevice: string = device;

  async function handleEntryClick(entry: DirEntry) {
    // Treat symlinks the same as directories: clicking a link runs
    // `ls -la <link>` on the device, which the kernel resolves to list
    // the target's contents. This is what `ls /sdcard` already does on
    // Android when /sdcard is a symlink to /storage/emulated/0.
    if (entry.kind !== 'dir' && entry.kind !== 'link') return;
    setLoading(true);
    setError(null);
    try {
      const list = await listRemoteDir(safeDevice, entry.path, asPkg, useRoot);
      setEntries(list);
      setCurrentPath(entry.path);
    } catch (e) {
      // Navigation failed — keep the prior listing visible so the user isn't
      // stranded, and surface *why* it failed via a toast (rather than the
      // full error card that replaces the table).
      toast.error(friendlyDirError(String(e), entry.path));
    } finally {
      setLoading(false);
    }
  }

  function friendlyDirError(raw: string, path: string): string {
    const lower = raw.toLowerCase();
    if (lower.includes('permission denied')) {
      return t('dataDir.openDirFailedPermission', { path });
    }
    if (lower.includes('not a directory')) {
      return t('dataDir.openDirFailedNotDir', { path });
    }
    // Device-level errors must be checked BEFORE the generic "not found"
    // branch so a dropped-device case ("device 'serial' not found",
    // "error: closed") is not misreported as a path-level miss.
    if (
      lower.includes('device offline') ||
      lower.includes('connection lost') ||
      lower.includes('error: closed') ||
      (lower.includes('device ') && lower.includes('not found'))
    ) {
      return t('dataDir.openDirFailedDevice', { path });
    }
    if (lower.includes('no such file') || lower.includes('not found')) {
      return t('dataDir.openDirFailedNotFound', { path });
    }
    return t('dataDir.openDirFailedGeneric', { path, error: raw });
  }

  async function handleUpload() {
    // Flip pushing=true immediately so the button gives feedback the instant
    // it is clicked, even if the native dialog takes a moment to appear.
    setPushing(true);
    let picked: string | null = null;
    try {
      const result = await openDialog({ multiple: false, directory: false });
      picked = typeof result === 'string' ? result : null;
    } catch (e) {
      // Most common cause: the data-dir webview window isn't listed in
      // capabilities, so the dialog plugin command is rejected silently.
      toast.error(
        t('dataDir.openDialogFailed', { error: String(e) })
      );
      setPushing(false);
      return;
    }
    if (!picked) {
      // User cancelled the picker — no error, just reset state.
      setPushing(false);
      return;
    }
    const fileName = picked.split(/[/\\]/).pop() ?? 'upload';
    try {
      const remotePath = currentPath.endsWith('/')
        ? `${currentPath}${fileName}`
        : `${currentPath}/${fileName}`;
      await pushFile(safeDevice, picked, remotePath, asPkg, useRoot);
      toast.success(t('dataDir.uploadSuccess', { name: fileName }));
      await refresh();
    } catch (e) {
      toast.error(t('dataDir.uploadFailed', { error: String(e) }));
    } finally {
      setPushing(false);
    }
  }

  async function handleDownload(entry: DirEntry) {
    if (entry.kind !== 'file' && entry.kind !== 'link') return;
    const defaultName = entry.name;
    setDownloadingPath(entry.path);
    let picked: string | null = null;
    try {
      picked = await saveDialog({ defaultPath: defaultName });
    } catch (e) {
      toast.error(
        t('dataDir.openSaveDialogFailed', { error: String(e) })
      );
      setDownloadingPath(null);
      return;
    }
    if (!picked) {
      setDownloadingPath(null);
      return;
    }
    try {
      const result = await pullFile(
        safeDevice,
        entry.path,
        picked,
        asPkg,
        useRoot,
      );
      toast.success(
        t('dataDir.downloadSuccess', {
          name: entry.name,
          detail: result,
          path: picked,
        })
      );
    } catch (e) {
      toast.error(t('dataDir.downloadFailed', { error: String(e) }));
    } finally {
      setDownloadingPath(null);
    }
  }

  async function confirmDelete() {
    if (!pendingDelete) return;
    setDeleting(true);
    try {
      await deleteRemoteFile(
        safeDevice,
        pendingDelete.path,
        asPkg,
        useRoot,
      );
      toast.success(t('dataDir.deleteSuccess', { name: pendingDelete.name }));
      setPendingDelete(null);
      await refresh();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setDeleting(false);
    }
  }

  const canGoUp = currentPath !== rootPath && currentPath.startsWith(rootPath);

  return (
    <div className="flex h-screen w-screen flex-col bg-bg-0 text-text-0">
      <header className="flex flex-wrap items-center justify-between gap-3 border-b border-border bg-bg-0 px-6 py-3">
        <div className="flex flex-wrap items-center gap-3">
          <h1 className="flex items-center gap-2 text-xl font-semibold text-text-0">
            <FolderOpen className="h-5 w-5 text-brand" />
            {t('dataDir.title')}
          </h1>
          <span className="inline-flex items-center gap-1.5 rounded-md border border-border bg-bg-1 px-2 py-1 font-mono text-xs text-text-1">
            {pkg}
          </span>
          <span className="inline-flex items-center gap-1.5 rounded-md border border-border bg-bg-1 px-2 py-1 font-mono text-xs text-text-2">
            {device}
          </span>
          {asPkg && (
            <Badge variant="success" className="text-[10px]">
              run-as
            </Badge>
          )}
        </div>
      </header>

      <main className="flex flex-1 flex-col gap-3 overflow-hidden p-6">
        <div className="flex flex-wrap items-center gap-2">
          <div className="flex min-w-0 flex-1 items-center gap-1.5 rounded-md border border-border bg-bg-1 px-3 py-1.5 font-mono text-xs text-text-1">
            <FolderIcon className="h-3 w-3 shrink-0 text-text-2" />
            <span className="truncate" title={currentPath}>
              {currentPath}
            </span>
          </div>
          {canGoUp && (
            <Button
              size="sm"
              variant="outline"
              onClick={() => {
                const parent =
                  currentPath.replace(/\/[^/]+\/?$/, '') || '/';
                void refresh(parent);
              }}
              title={t('dataDir.up')}
            >
              <ArrowUp className="h-3 w-3" />
              {t('dataDir.up')}
            </Button>
          )}
          <Button
            size="sm"
            variant="outline"
            onClick={() => void refresh()}
            disabled={loading}
          >
            {loading ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <RefreshCw className="h-3 w-3" />
            )}
            {t('dataDir.refresh')}
          </Button>
          <Button
            size="sm"
            onClick={() => void handleUpload()}
            disabled={loading || pushing || !!error}
          >
            {pushing ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Upload className="h-3 w-3" />
            )}
            {t('dataDir.upload')}
          </Button>
        </div>

        {error ? (
          <Card className="border-danger">
            <CardContent className="flex items-start gap-3 text-sm">
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-danger" />
              <div className="min-w-0 space-y-1">
                <div className="font-semibold text-text-0">
                  {t('dataDir.noAccess')}
                </div>
                <div className="break-all font-mono text-xs text-text-2">
                  {error}
                </div>
                <p className="pt-1 text-xs text-text-2">
                  {t('dataDir.noAccessDesc')}
                </p>
                <Button
                  size="sm"
                  variant="outline"
                  className="mt-2"
                  onClick={() => void refresh()}
                >
                  <RefreshCw className="h-3 w-3" />
                  {t('common.refresh')}
                </Button>
              </div>
            </CardContent>
          </Card>
        ) : loading && entries === null ? (
          <Card>
            <CardContent className="flex items-center gap-3 text-sm text-text-2">
              <Loader2 className="h-4 w-4 animate-spin text-brand" />
              {t('dataDir.loading')}
            </CardContent>
          </Card>
        ) : entries && entries.length === 0 ? (
          <Card>
            <CardContent className="text-sm text-text-2">
              {t('dataDir.empty')}
            </CardContent>
          </Card>
        ) : entries ? (
          <Card className="flex-1 overflow-hidden">
            <div className="max-h-full overflow-auto">
              <table className="w-full text-sm">
                <thead className="sticky top-0 bg-bg-1 text-xs uppercase tracking-wide text-text-2">
                  <tr>
                    <th className="px-3 py-2 text-left font-medium">
                      {t('dataDir.name')}
                    </th>
                    <th className="px-3 py-2 text-left font-medium">
                      {t('dataDir.permissions')}
                    </th>
                    <th className="px-3 py-2 text-right font-medium">
                      {t('dataDir.size')}
                    </th>
                    <th className="px-3 py-2 text-left font-medium">
                      {t('dataDir.modified')}
                    </th>
                    <th className="w-24 px-3 py-2 text-right font-medium">
                      {t('dataDir.actions')}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {entries.map((entry) => (
                    <EntryRow
                      key={entry.path}
                      entry={entry}
                      onOpen={() => void handleEntryClick(entry)}
                      onDelete={() => setPendingDelete(entry)}
                      onDownload={() => void handleDownload(entry)}
                      downloadingPath={downloadingPath}
                    />
                  ))}
                </tbody>
              </table>
            </div>
          </Card>
        ) : null}
      </main>

      <Dialog
        open={pendingDelete !== null}
        onOpenChange={(open) => {
          if (!open && !deleting) setPendingDelete(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {pendingDelete
                ? t('dataDir.confirmDeleteTitle', {
                    name: pendingDelete.name,
                  })
                : ''}
            </DialogTitle>
            <DialogDescription>
              {pendingDelete
                ? t('dataDir.confirmDeleteDesc', {
                    path: pendingDelete.path,
                  })
                : ''}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setPendingDelete(null)}
              disabled={deleting}
            >
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              onClick={() => void confirmDelete()}
              disabled={deleting}
            >
              {deleting && <Loader2 className="h-4 w-4 animate-spin" />}
              <Eraser className="h-4 w-4" />
              {t('dataDir.delete')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function EntryRow({
  entry,
  onOpen,
  onDelete,
  onDownload,
  downloadingPath,
}: {
  entry: DirEntry;
  onOpen: () => void;
  onDelete: () => void;
  onDownload: () => void;
  downloadingPath: string | null;
}) {
  const { t } = useTranslation();
  const Icon =
    entry.kind === 'dir'
      ? FolderIcon
      : entry.kind === 'link'
        ? LinkIcon
        : FileIcon;
  // Navigation-eligible: directories and symlinks-to-directories. Both
  // should respond to click, show hover, and use the brand accent color.
  // The on-device `ls` will resolve the symlink for us.
  const isNav = entry.kind === 'dir' || entry.kind === 'link';
  // For symlinks, `ls -la` reports the linknamelen as size — that's
  // almost never what the user wants to see, so we render `—` like dirs.
  const showSize = entry.kind !== 'dir' && entry.kind !== 'link';
  const rowTitle = entry.linkTarget
    ? t('dataDir.openLinkTooltip', { target: entry.linkTarget })
    : undefined;
  return (
    <tr
      className={cn(
        'border-t border-border transition-colors',
        isNav && 'cursor-pointer hover:bg-bg-2',
      )}
      onClick={isNav ? onOpen : undefined}
      title={rowTitle}
    >
      <td className="px-3 py-2">
        <div className="flex min-w-0 items-center gap-2">
          <Icon
            className={cn(
              'h-4 w-4 shrink-0',
              isNav ? 'text-brand' : 'text-text-2',
            )}
          />
          <span className="shrink-0" title={entry.name}>
            {entry.name}
          </span>
          {entry.linkTarget && (
            <span className="min-w-0 flex-1 truncate font-mono text-xs text-text-2">
              → {entry.linkTarget}
            </span>
          )}
        </div>
      </td>
      <td className="px-3 py-2 font-mono text-xs text-text-2">
        {entry.permissions}
      </td>
      <td className="px-3 py-2 text-right font-mono text-xs text-text-2">
        {showSize ? formatSize(entry.size) : '—'}
      </td>
      <td className="px-3 py-2 font-mono text-xs text-text-2">
        {entry.modified}
      </td>
      <td className="w-32 px-3 py-2 text-right">
        <div className="flex items-center justify-end gap-1">
          <Button
            size="sm"
            variant="ghost"
            onClick={(e) => {
              e.stopPropagation();
              onDownload();
            }}
            disabled={
              entry.kind === 'dir' || entry.kind === 'other'
            }
            title={t('dataDir.download')}
          >
            {entry.path === downloadingPath ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Download className="h-3 w-3" />
            )}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            onClick={(e) => {
              e.stopPropagation();
              onDelete();
            }}
            title={t('dataDir.delete')}
          >
            <Eraser className="h-3 w-3" />
          </Button>
        </div>
      </td>
    </tr>
  );
}

function CenteredMessage({
  title,
  description,
}: {
  title: string;
  description: string;
}) {
  return (
    <div className="grid h-screen w-screen place-items-center bg-bg-0 text-text-0">
      <div className="space-y-2 text-center">
        <div className="text-sm font-semibold text-text-0">{title}</div>
        <p className="text-xs text-text-2">{description}</p>
      </div>
    </div>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}
