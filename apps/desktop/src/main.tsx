import React from 'react';
import ReactDOM from 'react-dom/client';
import './dev-tauri-mock';
import { App } from './App';
import './styles/fonts.css';
import './styles/tokens.css';
import './styles/legacy-tokens.css';
import './styles/wotold.css';
import './styles/global.css';
import './ui/ui.css';
import './styles/pages.css';

const root = document.getElementById('root');
if (!root) throw new Error('root element missing');

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
