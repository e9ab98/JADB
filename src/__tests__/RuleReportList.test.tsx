import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { RuleReportList } from '@/features/rules/RuleReportList';
import type { RuleReport } from '@/ipc/rules';

const sample: RuleReport = {
  apk_path: '/tmp/a.apk',
  total_matched: 2,
  components: {
    native_libraries: [
      {
        name: 'libflutter.so',
        matched_rule: {
          rule_set_id: 'libchecker',
          rule_id: 'flutter',
          severity: 'info',
          description: 'Flutter runtime',
        },
      },
      { name: 'libfoo.so', matched_rule: null },
    ],
    activities: [
      {
        name: 'com.example.MainActivity',
        matched_rule: {
          rule_set_id: 'libchecker',
          rule_id: 'sample_act',
          severity: 'danger',
          description: 'Sample activity',
        },
      },
    ],
    services: [],
    receivers: [],
    providers: [],
  },
};

describe('<RuleReportList>', () => {
  it('renders the five APK component sections', () => {
    render(<RuleReportList report={sample} />);
    expect(screen.getByText('Native Libraries')).toBeInTheDocument();
    expect(screen.getByText('Activities')).toBeInTheDocument();
    expect(screen.getByText('Services')).toBeInTheDocument();
    expect(screen.getByText('Receivers')).toBeInTheDocument();
    expect(screen.getByText('Providers')).toBeInTheDocument();
  });

  it('lists every APK component and highlights matched ones', () => {
    render(<RuleReportList report={sample} />);
    expect(screen.getByText('libflutter.so')).toBeInTheDocument();
    expect(screen.getByText('libfoo.so')).toBeInTheDocument();
    expect(screen.getByText('com.example.MainActivity')).toBeInTheDocument();
  });

  it('renders empty state when no report', () => {
    render(<RuleReportList report={null} />);
    expect(screen.getByText(/No results/)).toBeInTheDocument();
  });
});
