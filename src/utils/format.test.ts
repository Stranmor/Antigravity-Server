import { describe, it, expect } from 'vitest';
import {
  formatBytes,
  getQuotaColor,
  formatCompactNumber,
} from './format';

describe('formatBytes', () => {
  it('returns "0 Bytes" for zero bytes', () => {
    expect(formatBytes(0)).toBe('0 Bytes');
  });

  it('formats bytes correctly', () => {
    expect(formatBytes(500)).toBe('500 Bytes');
  });

  it('formats kilobytes correctly', () => {
    expect(formatBytes(1024)).toBe('1 KB');
    expect(formatBytes(1536)).toBe('1.5 KB');
  });

  it('formats megabytes correctly', () => {
    expect(formatBytes(1048576)).toBe('1 MB');
    expect(formatBytes(1572864)).toBe('1.5 MB');
  });

  it('formats gigabytes correctly', () => {
    expect(formatBytes(1073741824)).toBe('1 GB');
  });
});

describe('getQuotaColor', () => {
  it('returns success for percentage >= 50', () => {
    expect(getQuotaColor(50)).toBe('success');
    expect(getQuotaColor(75)).toBe('success');
    expect(getQuotaColor(100)).toBe('success');
  });

  it('returns warning for percentage between 20 and 50', () => {
    expect(getQuotaColor(20)).toBe('warning');
    expect(getQuotaColor(35)).toBe('warning');
    expect(getQuotaColor(49)).toBe('warning');
  });

  it('returns error for percentage < 20', () => {
    expect(getQuotaColor(0)).toBe('error');
    expect(getQuotaColor(10)).toBe('error');
    expect(getQuotaColor(19)).toBe('error');
  });
});

describe('formatCompactNumber', () => {
  it('returns "0" for zero', () => {
    expect(formatCompactNumber(0)).toBe('0');
  });

  it('returns number as-is for values < 1000', () => {
    expect(formatCompactNumber(1)).toBe('1');
    expect(formatCompactNumber(999)).toBe('999');
    expect(formatCompactNumber(-500)).toBe('-500');
  });

  it('formats thousands with k suffix', () => {
    expect(formatCompactNumber(1000)).toBe('1k');
    expect(formatCompactNumber(1500)).toBe('1.5k');
    expect(formatCompactNumber(10000)).toBe('10k');
    expect(formatCompactNumber(999000)).toBe('999k');
  });

  it('formats millions with M suffix', () => {
    expect(formatCompactNumber(1000000)).toBe('1M');
    expect(formatCompactNumber(2500000)).toBe('2.5M');
  });

  it('formats billions with G suffix', () => {
    expect(formatCompactNumber(1000000000)).toBe('1G');
  });

  it('handles negative numbers', () => {
    expect(formatCompactNumber(-1000)).toBe('-1k');
    expect(formatCompactNumber(-1500000)).toBe('-1.5M');
  });
});
