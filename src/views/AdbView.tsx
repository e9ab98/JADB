import { useTranslation } from 'react-i18next';
import '@/i18n';
import { AdbConnectionPanel } from '@/features/adb/AdbConnectionPanel';

export function AdbView() {
  const { t } = useTranslation();

  return (
    <div className="space-y-4 p-6">
      <h1 className="text-2xl font-semibold text-text-0">{t('nav.adb')}</h1>
      <AdbConnectionPanel />
    </div>
  );
}
