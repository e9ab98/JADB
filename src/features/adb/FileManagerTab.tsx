import { useEffect, useRef, useState } from 'react';
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
  isDeviceRooted,
  listRemoteDir,
  pullFile,
  pushFile,
  type DirEntry,
} from '@/ipc/adb';
import { cn, formatBytes } from '@/lib/utils';

type Props = {
  serial: string;
  /**
   * Initial directory and the "up" boundary. The Up button is only enabled
   * while `currentPath` is strictly inside this prefix, so `/` keeps the
   * user from escaping the system root.
   */
  rootPath: string;
  /** Mirrors `listRemoteDir`'s `asPkg` — `run-as <pkg>` for debug apps. */
  asPkg?: string | null;
  /**
   * Mirrors `listRemoteDir`'s `useRoot`. Three states:
   *   - true  → always run as root via `su` (never fall back).
   *   - false → always run as the shell user.
   *   - null  → auto-detect: probe `isDeviceRooted` and prefer root when
   *             available, falling back to shell if the probe lies or the
   *             first `su`-flavored call errors out with permission denied.
   */
  useRoot?: boolean | null;
};

/**
 * Generic remote file manager tab. Used inside other views (e.g. AppsView's
 * "files" tab for browsing the system root) but also composable for any
 * (device, rootPath, mode) triple. The dedicated per-app "data-dir" window
 * keeps its own DataDirView so the OS-level window title remains specific.
 */
export function FileManagerTab({ serial, rootPath, asPkg = null, useRoot = null }: Props) {
  const { t } = useTranslation();
  const [currentPath, setCurrentPath] = useState<string>(rootPath);
  const [entries, setEntries] = useState<DirEntry[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<DirEntry | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [pushing, setPushing] = useState(false);
  const [downloadingPath, setDownloadingPath] = useState<string | null>(null);
  // Resolved elevation mode. `useRoot` is the caller hint; `useRootState`
  // is the actual mode used after the auto-detect probe (or the original
  // prop if the caller forced true/false). Set up in the probe effect.
  const [useRootState, setUseRootState] = useState<boolean>(true);
  const [probeDone, setProbeDone] = useState(false);
  // Becomes true after the first root→shell downgrade so we don't try
  // root again and again on every navigation.
  const [fallbackTried, setFallbackTried] = useState(false);

  // Pull the elevation mode through a ref so the closure inside `refresh`
  // always sees the latest value without re-binding the function.
  const useRootRef = useRef(useRootState);
  useRootRef.current = useRootState;

  async function refresh(path: string = currentPath) {
    setLoading(true);
    setError(null);
    // Capture the mode for this call; if we end up downgrading, the
    // fallback retry uses the new mode and the *next* refresh after that
    // will use the new state value via the ref.
    const mode = useRootRef.current;
    try {
      const list = await listRemoteDir(serial, path, asPkg, mode);
      setEntries(list);
      setCurrentPath(path);
      return;
    } catch (e) {
      const err = String(e);
      // Auto-detect mode: if the root probe said yes but `su` actually
      // failed (e.g. Magisk su got out of sync, or probing reported
      // test-keys on a userdebug build that doesn't actually have su),
      // retry once as the shell user instead of surfacing the error.
      // Subsequent calls skip the retry so we don't loop forever.
      if (mode && !fallbackTried) {
        setFallbackTried(true);
        setUseRootState(false);
        try {
          const list = await listRemoteDir(serial, path, asPkg, false);
          setEntries(list);
          setCurrentPath(path);
          return;
        } catch (e2) {
          setError(String(e2));
          setEntries(null);
          return;
        }
      }
      setError(err);
      setEntries(null);
    } finally {
      setLoading(false);
    }
  }

  // Resolve the elevation mode on mount (or whenever the caller flips
  // their explicit hint). If the caller pinned true/false we honor it
  // and skip the probe; otherwise we ask the device and prefer root.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      if (useRoot === true) {
        if (!cancelled) {
          setUseRootState(true);
          setProbeDone(true);
        }
        return;
      }
      if (useRoot === false) {
        if (!cancelled) {
          setUseRootState(false);
          setProbeDone(true);
        }
        return;
      }
      // useRoot === null/undefined → detect.
      let rooted = false;
      try {
        rooted = await isDeviceRooted(serial);
      } catch {
        rooted = false;
      }
      if (cancelled) return;
      setUseRootState(rooted);
      setProbeDone(true);
    })();
    return () => { cancelled = true; };
  }, [serial, useRoot]);

  // Re-list when the caller swaps the device, rootPath, or after the
  // elevation probe resolves for the first time.
  useEffect(() => {
    if (!probeDone) return;
    void refresh(rootPath);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serial, asPkg, useRootState, rootPath, probeDone]);

  async function handleEntryClick(entry: DirEntry) {
    if (entry.kind !== 'dir') return;
    setLoading(true);
    setError(null);
    const mode = useRootRef.current;
    try {
      const list = await listRemoteDir(serial, entry.path, asPkg, mode);
      setEntries(list);
      setCurrentPath(entry.path);
    } catch (e) {
      // Navigation failed — keep the prior listing visible so the user
      // isn't stranded, and surface *why* it failed via a toast instead of
      // the full error card that would replace the table.
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
    setPushing(true);
    let picked: string | null = null;
    try {
      const result = await openDialog({ multiple: false, directory: false });
      picked = typeof result === 'string' ? result : null;
    } catch (e) {
      toast.error(t('dataDir.openDialogFailed', { error: String(e) }));
      setPushing(false);
      return;
    }
    if (!picked) {
      setPushing(false);
      return;
    }
    const fileName = picked.split(/[/\\]/).pop() ?? 'upload';
    try {
      const remotePath = currentPath.endsWith('/')
        ? `${currentPath}${fileName}`
        : `${currentPath}/${fileName}`;
      await pushFile(serial, picked, remotePath, asPkg, useRootRef.current);
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
      toast.error(t('dataDir.openSaveDialogFailed', { error: String(e) }));
      setDownloadingPath(null);
      return;
    }
    if (!picked) {
      setDownloadingPath(null);
      return;
    }
    try {
      const result = await pullFile(serial, entry.path, picked, asPkg, useRootRef.current);
      toast.success(
        t('dataDir.downloadSuccess', {
          name: entry.name,
          detail: result,
          path: picked,
        }),
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
      await deleteRemoteFile(serial, pendingDelete.path, asPkg, useRootRef.current);
      toast.success(t('dataDir.deleteSuccess', { name: pendingDelete.name }));
      setPendingDelete(null);
      await refresh();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setDeleting(false);
    }
  }

  // Only allow Up while we are still inside the boundary. This keeps the
  // user from jumping past rootPath when starting deeper (e.g. /data/data).
  const canGoUp =
    currentPath !== rootPath &&
    (rootPath === '/' ? currentPath !== '/' : currentPath.startsWith(rootPath));

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex min-w-0 flex-1 items-center gap-1.5 rounded-md border border-border bg-bg-1 px-3 py-1.5 font-mono text-xs text-text-1">
          <FolderIcon className="h-3 w-3 shrink-0 text-text-2" />
          <span className="truncate" title={currentPath}>
            {currentPath}
          </span>
        </div>
        {asPkg && (
          <Badge variant="success" className="text-[10px]">
            run-as
          </Badge>
        )}
        {useRootState && (
          <Badge variant="warning" className="text-[10px]">
            root
          </Badge>
        )}
        {canGoUp && (
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              const parent = currentPath.replace(/\/[^/]+\/?$/, '') || '/';
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
              <Button
                size="sm"
                variant="outline"
                className="mt-2"
                onClick={() => void refresh()}
              >
                <RefreshCw className="h-3 w-3" />
                {t('dataDir.refresh')}
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
                  <th className="w-32 px-3 py-2 text-right font-medium">
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
                ? t('dataDir.confirmDeleteTitle', { name: pendingDelete.name })
                : ''}
            </DialogTitle>
            <DialogDescription>
              {pendingDelete
                ? t('dataDir.confirmDeleteDesc', { path: pendingDelete.path })
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
  const isDir = entry.kind === 'dir';
  return (
    <tr
      className={cn(
        'border-t border-border transition-colors',
        isDir && 'cursor-pointer hover:bg-bg-2',
      )}
      onClick={isDir ? onOpen : undefined}
    >
      <td className="px-3 py-2">
        <div className="flex min-w-0 items-center gap-2">
          <Icon
            className={cn(
              'h-4 w-4 shrink-0',
              isDir ? 'text-brand' : 'text-text-2',
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
        {isDir ? '—' : formatBytes(entry.size)}
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
            disabled={entry.kind === 'dir' || entry.kind === 'other'}
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
