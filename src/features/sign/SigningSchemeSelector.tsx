import { useId } from 'react';
import { useTranslation } from 'react-i18next';
import type { SigningSchemes } from '@/types/signing';

type Props = {
  value: SigningSchemes;
  onChange: (value: SigningSchemes) => void;
  disabled?: boolean;
};

export function SigningSchemeSelector({ value, onChange, disabled = false }: Props) {
  const { t } = useTranslation();
  const prefix = useId();

  function setV1(checked: boolean) {
    onChange({ ...value, v1: checked });
  }

  function setV2(checked: boolean) {
    onChange({ ...value, v2: checked });
  }

  function setV3(checked: boolean) {
    onChange({
      ...value,
      v3: checked,
      v2: checked ? true : value.v2,
    });
  }

  function setV4(checked: boolean) {
    onChange({
      ...value,
      v4: checked,
      v2: checked ? true : value.v2,
    });
  }

  return (
    <fieldset className="rounded-xl border border-border bg-bg-2/40 p-3" disabled={disabled}>
      <legend className="px-1 text-sm font-medium text-text-0">{t('sign.schemesTitle')}</legend>
      <p className="mb-3 text-xs text-text-2">{t('sign.schemesHint')}</p>
      <div className="grid gap-2 sm:grid-cols-2">
        <SchemeOption
          id={`${prefix}-v1`}
          label={t('sign.schemeV1')}
          description={t('sign.schemeV1Desc')}
          checked={value.v1}
          disabled={disabled}
          onChange={setV1}
        />
        <SchemeOption
          id={`${prefix}-v2`}
          label={t('sign.schemeV2')}
          description={t('sign.schemeV2Desc')}
          checked={value.v2}
          disabled={disabled}
          onChange={setV2}
        />
        <SchemeOption
          id={`${prefix}-v3`}
          label={t('sign.schemeV3')}
          description={t('sign.schemeV3Desc')}
          checked={value.v3}
          disabled={disabled}
          onChange={setV3}
        />
        <SchemeOption
          id={`${prefix}-v4`}
          label={t('sign.schemeV4')}
          description={t('sign.schemeV4Desc')}
          checked={value.v4}
          disabled={disabled}
          onChange={setV4}
        />
      </div>
    </fieldset>
  );
}

type SchemeOptionProps = {
  id: string;
  label: string;
  description: string;
  checked: boolean;
  disabled: boolean;
  onChange: (checked: boolean) => void;
};

function SchemeOption({
  id,
  label,
  description,
  checked,
  disabled,
  onChange,
}: SchemeOptionProps) {
  return (
    <label
      htmlFor={id}
      className="flex items-start gap-2 rounded-lg border border-border bg-bg-1 p-2.5"
    >
      <input
        id={id}
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
        className="mt-0.5 h-4 w-4 shrink-0 accent-brand disabled:opacity-60"
      />
      <span className="min-w-0">
        <span className="block text-sm font-medium text-text-0">{label}</span>
        <span className="block text-xs leading-relaxed text-text-2">{description}</span>
      </span>
    </label>
  );
}
