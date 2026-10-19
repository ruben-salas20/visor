<script lang="ts">
	import { ScrollArea } from '$lib/components/ui/scroll-area';
	import { estado } from '$lib/marco.svelte';
	import Carpeta from './Carpeta.svelte';

	// The tree arrives as a flat list of "carpeta/archivo"; group it by folder, keeping its order.
	const grupos = $derived.by(() => {
		const mapa = new Map<string, string[]>();
		for (const ruta of estado.arbol) {
			const carpeta = ruta.split('/')[0];
			const lista = mapa.get(carpeta);
			if (lista) lista.push(ruta);
			else mapa.set(carpeta, [ruta]);
		}
		return [...mapa];
	});
</script>

<aside id="arbol" class="flex h-full w-[260px] shrink-0 flex-col border-r bg-sidebar">
	<div class="px-4 pt-3 pb-2 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">Bóveda</div>
	<ScrollArea class="min-h-0 flex-1">
		<div class="flex flex-col gap-0.5 px-2 pb-3">
			{#each grupos as [carpeta, archivos] (carpeta)}
				<Carpeta nombre={carpeta} {archivos} />
			{/each}
		</div>
	</ScrollArea>
</aside>
