import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { toast } from 'sonner';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
  DialogClose,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';

export type ImportLineagePayload = {
  label: string;
  srcPath: string;
  oldSignatureId: string;
  newSignatureId: string;
};

type SignatureOption = {
  id: string;
  label: string;
};

type Props = {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  signatures: SignatureOption[];
  onSubmit: (data: ImportLineagePayload) => Promise<void>;
};

export function ImportLineageDialog({ open, onOpenChange, signatures, onSubmit }: Props) {
  const { t } = useTranslation();
  const [label, setLabel] = useState('');
  const [srcPath, setSrcPath] = useState('');
  const [oldId, setOldId] = useState('');
  const [newId, setNewId] = useState('');
  const [busy, setBusy] = useState(false);
  const invalid =
    !srcPath.trim() || !oldId || !newId || oldId === newId || !label.trim();

  async function pickFile() {
    const p = await openDialog({
      multiple: false,
      filters: [{ name: 'Lineage', extensions: ['lineage'] }],
    });
    if (typeof p === 'string') setSrcPath(p);
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('lineages.importTitle')}</DialogTitle>
          <DialogDescription>{t('lineages.importDescription')}</DialogDescription>
        </DialogHeader>
        <form
          className="grid gap-3"
          onSubmit={async (e) => {
            e.preventDefault();
            if (invalid) return;
            setBusy(true);
            try {
              await onSubmit({ label, srcPath, oldSignatureId: oldId, newSignatureId: newId });
              onOpenChange(false);
              setLabel('');
              setSrcPath('');
              setOldId('');
              setNewId('');
            } catch (err) {
              console.error('import lineage failed', err);
              toast.error(t('lineages.importFailed', { error: String(err) }));
            } finally {
              setBusy(false);
            }
          }}
        >
          <div className="flex gap-2">
            <Input
              readOnly
              value={srcPath}
              placeholder={t('lineages.fieldLineageFile')}
            />
            <Button type="button" variant="outline" onClick={pickFile}>
              {t('common.open')}
            </Button>
          </div>
          <Input
            placeholder={t('lineages.fieldLabel')}
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            required
          />
          <label className="grid gap-1 text-xs text-text-2">
            <span>{t('lineages.fieldOld')}</span>
            <select
              value={oldId}
              onChange={(e) => setOldId(e.target.value)}
              className="h-9 rounded-md border border-border bg-bg-1 px-2 text-sm text-text-0"
              required
            >
              <option value="">…</option>
              {signatures.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.label}
                </option>
              ))}
            </select>
          </label>
          <label className="grid gap-1 text-xs text-text-2">
            <span>{t('lineages.fieldNew')}</span>
            <select
              value={newId}
              onChange={(e) => setNewId(e.target.value)}
              className="h-9 rounded-md border border-border bg-bg-1 px-2 text-sm text-text-0"
              required
            >
              <option value="">…</option>
              {signatures.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.label}
                </option>
              ))}
            </select>
          </label>
          {oldId && newId && oldId === newId && (
            <p className="text-xs text-amber-300">{t('lineages.sameSignatureWarning')}</p>
          )}
          <DialogFooter>
            <DialogClose asChild>
              <Button type="button" variant="outline">
                {t('common.cancel')}
              </Button>
            </DialogClose>
            <Button type="submit" disabled={busy || invalid}>
              {busy ? '…' : t('common.save')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
