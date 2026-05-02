import React from 'react';
import ReactDOM from 'react-dom/client';
import { IslandWindow } from './island/IslandWindow';
import { initIslandI18n } from './island/i18n';

import '@fontsource-variable/inter';
import './styles/nothing/variables.css';
import './styles/nothing/reset.css';
import './styles/nothing/typography.css';
import './styles/nothing/animations.css';
import './styles/nothing/island.css';

document.documentElement.setAttribute('data-theme', 'dark');
initIslandI18n();

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <IslandWindow />
  </React.StrictMode>
);
