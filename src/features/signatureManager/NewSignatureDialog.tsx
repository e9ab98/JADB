import { useMemo, useState } from 'react';
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
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import {
  KEY_ALGORITHM_OPTIONS,
  KEY_SIZE_OPTIONS,
  type NewKeystoreDName,
  type NewKeystoreInput,
} from '@/ipc/signatures';

export type NewSignaturePayload = NewKeystoreInput;

type Props = {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  onSubmit: (data: NewSignaturePayload) => Promise<void>;
};

const DEFAULT_VALIDITY_YEARS = 30;

function yearsToDays(years: number): number {
  return Math.max(1, Math.round(years * 365));
}

export function NewSignatureDialog({ open, onOpenChange, onSubmit }: Props) {
  const { t } = useTranslation();
  const [label, setLabel] = useState('');
  const [alias, setAlias] = useState('');
  const [keystorePassword, setKeystorePassword] = useState('');
  const [keyPassword, setKeyPassword] = useState('');
  const [keyAlgorithm, setKeyAlgorithm] =
    useState<(typeof KEY_ALGORITHM_OPTIONS)[number]>('RSA');
  const [keySize, setKeySize] = useState<number>(2048);
  const [validityYears, setValidityYears] = useState<number>(DEFAULT_VALIDITY_YEARS);
  const [dname, setDname] = useState<NewKeystoreDName>({});
  const [busy, setBusy] = useState(false);

  const sizeOptions = useMemo(() => KEY_SIZE_OPTIONS[keyAlgorithm], [keyAlgorithm]);

  function reset() {
    setLabel('');
    setAlias('');
    setKeystorePassword('');
    setKeyPassword('');
    setKeyAlgorithm('RSA');
    setKeySize(2048);
    setValidityYears(DEFAULT_VALIDITY_YEARS);
    setDname({});
  }

  async function onSubmitForm(e: React.FormEvent) {
    e.preventDefault();
    // Java 的 keytool 在 JKS 格式下虽然容忍两个密码不同,
    // 但下游 Android Studio / apksigner 在 keystore 加载阶段
    // 通常只提示一次,用户极易混淆 keyPassword 和 keystorePassword。
    // 这里强制一致,避免生成成功却用不上的"假成功"流程。
    if (keystorePassword !== keyPassword) {
      toast.error(t('signatures.passwordMismatch'));
      return;
    }
    setBusy(true);
    try {
      await onSubmit({
        label,
        alias,
        keystorePassword,
        keyPassword,
        options: {
          keyAlgorithm,
          keySize,
          validityDays: yearsToDays(validityYears),
          dname,
        },
      });
      onOpenChange(false);
      reset();
    } catch (err) {
      console.error('create new keystore failed', err);
      toast.error(t('signatures.createFailed', { error: String(err) }));
    } finally {
      setBusy(false);
    }
  }

  function updateDname(key: keyof NewKeystoreDName, value: string) {
    setDname((prev) => {
      const next = { ...prev };
      if (value.trim() === '') {
        delete next[key];
      } else {
        next[key] = value;
      }
      return next;
    });
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('signatures.newTitle')}</DialogTitle>
          <DialogDescription>{t('signatures.newDescription')}</DialogDescription>
        </DialogHeader>
        <form className="grid gap-3" onSubmit={onSubmitForm}>
          <Input
            placeholder={t('signatures.fieldLabel')}
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            required
          />
          <Input
            placeholder={t('signatures.fieldAliasPlaceholder')}
            value={alias}
            onChange={(e) => setAlias(e.target.value)}
            required
          />
          <Input
            type="password"
            placeholder={t('signatures.fieldKeystorePwd')}
            value={keystorePassword}
            onChange={(e) => setKeystorePassword(e.target.value)}
            required
          />
          <Input
            type="password"
            placeholder={t('signatures.fieldKeyPwd')}
            value={keyPassword}
            onChange={(e) => setKeyPassword(e.target.value)}
            required
          />

          <div className="grid gap-2 sm:grid-cols-3">
            <Field label={t('signatures.fieldKeyAlg')}>
              <select
                value={keyAlgorithm}
                onChange={(e) => {
                  const next = e.target.value as (typeof KEY_ALGORITHM_OPTIONS)[number];
                  setKeyAlgorithm(next);
                  const allowed = KEY_SIZE_OPTIONS[next];
                  // noUncheckedIndexedAccess makes allowed[0] `number | undefined`.
                  // Fall back to 2048 (the same default useState is seeded with)
                  // if the algorithm has no permitted sizes (defensive; shouldn't happen).
                  if (!allowed.includes(keySize)) setKeySize(allowed[0] ?? 2048);
                }}
                className="h-9 rounded-md border border-border bg-bg-1 px-2 text-sm text-text-0"
              >
                {KEY_ALGORITHM_OPTIONS.map((opt) => (
                  <option key={opt} value={opt}>
                    {opt}
                  </option>
                ))}
              </select>
            </Field>
            <Field label={t('signatures.fieldKeySize')}>
              <select
                value={keySize}
                onChange={(e) => setKeySize(Number(e.target.value))}
                className="h-9 rounded-md border border-border bg-bg-1 px-2 text-sm text-text-0"
              >
                {sizeOptions.map((opt) => (
                  <option key={opt} value={opt}>
                    {opt}
                  </option>
                ))}
              </select>
            </Field>
            <Field label={t('signatures.fieldValidity')}>
              <Input
                type="number"
                min={1}
                max={100}
                value={validityYears}
                onChange={(e) => setValidityYears(Number(e.target.value) || 1)}
                required
              />
            </Field>
          </div>

          <fieldset className="rounded-md border border-border bg-bg-2/40 p-3">
            <legend className="px-1 text-xs text-text-2">
              {t('signatures.fieldDName')}
            </legend>
            <p className="mb-2 text-xs text-text-2">{t('signatures.dnameHint')}</p>
            <div className="grid gap-2 sm:grid-cols-2">
              <DNameField
                label="CN"
                value={dname.cn ?? ''}
                onChange={(v) => updateDname('cn', v)}
              />
              <DNameField
                label="OU"
                value={dname.ou ?? ''}
                onChange={(v) => updateDname('ou', v)}
              />
              <DNameField
                label="O"
                value={dname.o ?? ''}
                onChange={(v) => updateDname('o', v)}
              />
              <DNameField
                label="L"
                value={dname.l ?? ''}
                onChange={(v) => updateDname('l', v)}
              />
              <DNameField
                label="ST"
                value={dname.st ?? ''}
                onChange={(v) => updateDname('st', v)}
              />
              <DNameField
                label="C"
                value={dname.c ?? ''}
                onChange={(v) => updateDname('c', v)}
              />
            </div>
          </fieldset>

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

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="grid gap-1 text-xs text-text-2">
      <span>{label}</span>
      {children}
    </label>
  );
}

function DNameField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <Input
      placeholder={`${label} (${t('signatures.optional')})`}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

