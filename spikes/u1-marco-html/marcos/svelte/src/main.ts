import './app.css';
import { mount } from 'svelte';
import App from './App.svelte';
import { iniciar } from '$lib/marco.svelte';

mount(App, { target: document.getElementById('app')! });
iniciar();
