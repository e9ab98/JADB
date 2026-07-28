import { act } from '@testing-library/react';
import { describe, it, expect, beforeEach } from 'vitest';
import { useTasksStore } from '@/store/task';

describe('useTasksStore', () => {
  beforeEach(() => useTasksStore.setState({ tasks: {} }));

  it('setTask creates a running task', () => {
    act(() => useTasksStore.getState().setTask('t1', { percent: 50, stage: 'decoding' }));
    const t = useTasksStore.getState().tasks['t1'];
    expect(t?.percent).toBe(50);
    expect(t?.stage).toBe('decoding');
    expect(t?.status).toBe('running');
  });

  it('appendLog accumulates logs', () => {
    act(() => useTasksStore.getState().setTask('t1', { percent: 0, stage: '' }));
    act(() => useTasksStore.getState().appendLog({ task_id: 't1', line: 'hello', level: 'info' }));
    act(() => useTasksStore.getState().appendLog({ task_id: 't1', line: 'world', level: 'warn' }));
    const t1 = useTasksStore.getState().tasks['t1']!;
    expect(t1.logs).toHaveLength(2);
    expect(t1.logs[1]?.level).toBe('warn');
  });

  it('applyProgress updates percent and stage', () => {
    act(() => useTasksStore.getState().setTask('t2', { percent: 0, stage: 'starting' }));
    act(() => useTasksStore.getState().applyProgress({ task_id: 't2', percent: 75, stage: 'flushing' }));
    const t = useTasksStore.getState().tasks['t2'];
    expect(t?.percent).toBe(75);
    expect(t?.stage).toBe('flushing');
  });

  it('applyDone flips status to done and stores result', () => {
    act(() => useTasksStore.getState().setTask('t3', { percent: 50, stage: 'work' }));
    act(() => useTasksStore.getState().applyDone({ task_id: 't3', result: '/tmp/out' }));
    const t = useTasksStore.getState().tasks['t3'];
    expect(t?.status).toBe('done');
    expect(t?.percent).toBe(100);
    expect(t?.result).toBe('/tmp/out');
  });

  it('applyError flips status to error', () => {
    act(() => useTasksStore.getState().setTask('t4', { percent: 10, stage: 'work' }));
    act(() => useTasksStore.getState().applyError({ task_id: 't4', error: 'something broke' }));
    const t = useTasksStore.getState().tasks['t4'];
    expect(t?.status).toBe('error');
    expect(t?.error).toBe('something broke');
  });

  it('applyError with cancellation message flips status to cancelled', () => {
    act(() => useTasksStore.getState().setTask('t5', { percent: 10, stage: 'work' }));
    act(() => useTasksStore.getState().applyError({ task_id: 't5', error: '任务已取消' }));
    expect(useTasksStore.getState().tasks['t5']?.status).toBe('cancelled');
  });
});
