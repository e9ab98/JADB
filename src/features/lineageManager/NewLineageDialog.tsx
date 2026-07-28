import { useState } from 'react';
import { useTranslation } from 'react-i18next';
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

export type NewLineagePayload = {
  label: string;
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
  onSubmit: (data: NewLineagePayload) => Promise<void>;
};

export function NewLineageDialog({ open, onOpenChange, signatures, onSubmit }: Props) {
  const { t } = useTranslation();
  const [label, setLabel] = useState('');
  const [oldId, setOldId] = useState('');
  const [newId, setNewId] = useState('');
  const [busy, setBusy] = useState(false);
  const invalid = !oldId || !newId || oldId === newId || !label.trim();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('lineages.newTitle')}</DialogTitle>
          <DialogDescription>{t('lineages.newDescription')}</DialogDescription>
        </DialogHeader>
        <form
          className="grid gap-3"
          onSubmit={async (e) => {
            e.preventDefault();
            if (invalid) return;
            setBusy(true);
            try {
              await onSubmit({ label, oldSignatureId: oldId, newSignatureId: newId });
              onOpenChange(false);
              setLabel('');
              setOldId('');
              setNewId('');
            } catch (err) {
              console.error('create lineage failed', err);
              toast.error(t('lineages.createFailed', { error: String(err) }));
            } finally {
              setBusy(false);
            }
          }}
        >
          <Input
            placeholder={t('lineages.fieldLabel')}
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            required
          />
          <SignaturePicker
            label={t('lineages.fieldOld')}
            value={oldId}
            onChange={setOldId}
            signatures={signatures}
          />
          <SignaturePicker
            label={t('lineages.fieldNew')}
            value={newId}
            onChange={setNewId}
            signatures={signatures}
          />
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

function SignaturePicker({
  label,
  value,
  onChange,
  signatures,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  signatures: SignatureOption[];
}) {
  return (
    <label className="grid gap-1 text-xs text-text-2">
      <span>{label}</span>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
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
  );
}
