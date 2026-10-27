import { mount } from 'svelte';
import Panel from '../../src/components/Panel.svelte';

const target = document.getElementById('app');
if (!target) throw new Error('panel root element is missing');
mount(Panel, { target });
