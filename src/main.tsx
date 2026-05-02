import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { getStoredTheme } from './theme/themeOptions';

import '@fontsource-variable/inter';
import './styles/nothing/variables.css';
import './styles/nothing/reset.css';
import './styles/nothing/typography.css';
import './styles/nothing/components.css';
import './styles/nothing/animations.css';
import './styles/nothing/music.css';
import './styles/nothing/island.css';

// Apply stored theme before React mounts to prevent flash of wrong theme
document.documentElement.setAttribute('data-theme', getStoredTheme(window.localStorage));

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
