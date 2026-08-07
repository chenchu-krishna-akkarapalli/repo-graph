import React from 'react';
import { useTheme } from '../hooks/useTheme';

export function Button() {
  const theme = useTheme();
  return <button className={theme}>ok</button>;
}
