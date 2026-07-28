import { render, screen } from '@testing-library/react';
import { describe, it, expect, beforeEach } from 'vitest';
import { TaskPanel } from '@/components/TaskPanel';
import { useTasksStore } from '@/store/task';

describe('<TaskPanel>', () => {
  beforeEach(() => useTasksStore.setState({ tasks: {} }));

  it('shows empty state when no task', () => {
    render(<TaskPanel taskId={null} />);
    expect(screen.getByText(/取消|Cancel/)).toBeInTheDocument();
  });

  it('renders logs and stage for a running task', () => {
    useTasksStore.getState().setTask('t1', { percent: 30, stage: 'decoding' });
    useTasksStore.getState().appendLog({ task_id: 't1', line: 'starting apktool', level: 'info' });
    render(<TaskPanel taskId="t1" />);
    expect(screen.getByText(/starting apktool/)).toBeInTheDocument();
    expect(screen.getByText(/decoding/)).toBeInTheDocument();
  });

  it('shows error banner for failed tasks', () => {
    useTasksStore.getState().setTask('t2', { percent: 10, stage: 'work' });
    useTasksStore.getState().applyError({ task_id: 't2', error: 'apktool not found' });
    render(<TaskPanel taskId="t2" />);
    expect(screen.getByText(/apktool not found/)).toBeInTheDocument();
  });
});
