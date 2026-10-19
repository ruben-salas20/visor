# Experimento L5: paquetes .deb y Flatpak

Pregunta: ¿arranca visor instalado como paquete .deb y como Flatpak?
Es una prueba de viabilidad. No publica nada: no crea releases, etiquetas ni PR.

## Qué hay aquí

| Archivo | Para qué sirve |
|---|---|
| `io.github.ruben_salas20.visor.yml` | Manifiesto de Flatpak (receta para compilar la app en una sandbox). |
| `visor.desktop` | Entrada de menú que se empaqueta en el .deb y en el Flatpak. |
| `deb-install.sh` | Instala el .deb en una distro limpia y mide el resultado. |
| `pruebas.sh` | Pruebas a–f sobre una copia instalada (`deb` o `flatpak`). |

El flujo completo está en `.github/workflows/linux-packages.yml`. Se ejecuta solo en GitHub Actions.

## Cómo se ejecuta

1. En GitHub, abrir la pestaña Actions y lanzar el flujo `linux-packages` (workflow_dispatch).
2. Los resultados aparecen en el resumen de cada ejecución y en los artefactos `res-*`.

## Qué se mide

- Tamaño del .deb, del binario y del bundle de Flatpak.
- Paquetes nuevos que instala apt en cada distro.
- Mediana de 10 llamadas a `visor status`, medida desde fuera.
- Pruebas a–f: arranque, abrir una nota, CLI, servidor MCP, archivo de escritorio, aviso de actualización y una carpeta en `/tmp`.

## Límites conocidos

- El manifiesto da acceso a la red durante la compilación (`--share=network` en `build-args`). Es solo para el experimento: un paquete real debería incluir las dependencias de Rust.
- La versión del runtime de GNOME la elige el flujo (la más nueva estable), no el archivo.
