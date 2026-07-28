import type { PropsWithChildren } from 'react';
import { useTheme } from '@/hooks/useTheme';

export function ThemeProvider({ children }: PropsWithChildren) {
  useTheme();
  return <>{children}</>;
}
