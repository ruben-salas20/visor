<script lang="ts">
	import * as Collapsible from '$lib/components/ui/collapsible';
	import { abrirDesdeArbol, marcarArrastre } from '$lib/marco.svelte';
	import { ChevronRight, FileText, Folder } from '@lucide/svelte';

	let { nombre, archivos }: { nombre: string; archivos: string[] } = $props();
	let abierta = $state(true);
</script>

<Collapsible.Root bind:open={abierta}>
	<Collapsible.Trigger
		class="group flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-xs font-semibold text-muted-foreground transition-colors outline-none hover:bg-muted hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
	>
		<ChevronRight class="size-3.5 shrink-0 transition-transform group-data-[state=open]:rotate-90" />
		<Folder class="size-3.5 shrink-0" />
		<span class="truncate">{nombre}</span>
		<span class="ml-auto font-normal tabular-nums text-[11px] opacity-70">{archivos.length}</span>
	</Collapsible.Trigger>
	<Collapsible.Content class="flex flex-col gap-px pt-0.5 pb-1 pl-3">
		{#each archivos as ruta (ruta)}
			<div
				role="button"
				tabindex="0"
				draggable="true"
				data-ruta={ruta}
				class="item flex cursor-pointer items-center gap-2 rounded-md px-2 py-1 text-[13px] text-foreground/90 transition-colors select-none outline-none hover:bg-accent hover:text-accent-foreground focus-visible:ring-2 focus-visible:ring-ring active:cursor-grabbing"
				onclick={() => abrirDesdeArbol(ruta)}
				onkeydown={(e) => {
					if (e.key === 'Enter' || e.key === ' ') {
						e.preventDefault();
						abrirDesdeArbol(ruta);
					}
				}}
				onmousedown={() => marcarArrastre(ruta)}
				ondragstart={(e) => {
					e.dataTransfer?.setData('text/plain', ruta);
					if (e.dataTransfer) e.dataTransfer.effectAllowed = 'copy';
				}}
			>
				<FileText class="size-3.5 shrink-0 text-muted-foreground" />
				<span class="truncate">{ruta.slice(nombre.length + 1)}</span>
			</div>
		{/each}
	</Collapsible.Content>
</Collapsible.Root>
