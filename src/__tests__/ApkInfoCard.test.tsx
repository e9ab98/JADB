import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { ApkInfoCard } from '@/features/apkAnalyze/ApkInfoCard';
import type { ApkInfo } from '@/ipc/analyze';

const sample: ApkInfo = {
  package_name: 'com.example.app',
  version_code: '42',
  version_name: '1.2.3',
  min_sdk: '24',
  target_sdk: '34',
  max_sdk: null,
  application_label: 'Example',
  permissions: ['android.permission.INTERNET'],
  activities: ['com.example.app.MainActivity'],
  services: [],
  receivers: [],
  providers: [],
  raw_badging: '',
};

describe('<ApkInfoCard>', () => {
  it('shows package and version', () => {
    render(<ApkInfoCard info={sample} />);
    expect(screen.getByText('com.example.app')).toBeInTheDocument();
    expect(screen.getByText('42')).toBeInTheDocument();
  });

  it('shows permissions badge', () => {
    render(<ApkInfoCard info={sample} />);
    expect(screen.getByText(/android\.permission\.INTERNET/)).toBeInTheDocument();
  });

  it('shows activity in components section', () => {
    render(<ApkInfoCard info={sample} />);
    expect(screen.getByText(/com\.example\.app\.MainActivity/)).toBeInTheDocument();
  });
});
