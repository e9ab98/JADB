import { useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogClose,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';

export type ImportKeystorePayload = {
  srcPath: string;
  alias: string;
  password: string;
  label: string;
};

type Props = {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  onSubmit: (data: ImportKeystorePayload) => Promise<void>;
};

export function ImportKeystoreDialog({ open, onOpenChange, onSubmit }: Props) {
  const { t } = useTranslation();
  const [srcPath, setSrcPath] = useState('');
  const [alias, setAlias] = useState('');
  const [password, setPassword] = useState('');
  const [label, setLabel] = useState('');
  const [busy, setBusy] = useState(false);

  async function pickFile() {
    const p = await openDialog({
      multiple: false,
      filters: [{ name: 'Keystore', extensions: ['jks', 'keystore'] }],
    });
    if (typeof p === 'string') setSrcPath(p);
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('signatures.importTitle')}</DialogTitle>
        </DialogHeader>
        <form
          className="grid gap-3"
          onSubmit={async (e) => {
            e.preventDefault();
            setBusy(true);
            try {
              await onSubmit({ srcPath, alias, password, label });
              onOpenChange(false);
              setSrcPath('');
              setAlias('');
              setPassword('');
              setLabel('');
            } finally {
              setBusy(false);
            }
          }}
        >
          <div className="flex gap-2">
            <Input
              readOnly
              value={srcPath}
              placeholder={t('signatures.fieldKeystore')}
            />
            <Button type="button" variant="outline" onClick={pickFile}>
              {t('common.open')}
            </Button>
          </div>
          <Input
            placeholder={t('signatures.fieldAlias')}
            value={alias}
            onChange={(e) => setAlias(e.target.value)}
            required
          />
          <Input
            type="password"
            placeholder={t('signatures.fieldKeystorePwd')}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
          />
          <Input
            placeholder={t('signatures.fieldLabel')}
            value={label}
            onChange={(e) => setLabel(e.target.value)}
          />
          <DialogFooter>
            <DialogClose asChild>
              <Button type="button" variant="outline">
                {t('common.cancel')}
              </Button>
            </DialogClose>
            <Button type="submit" disabled={busy}>
              {busy ? '…' : t('common.save')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
