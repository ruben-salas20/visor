# 0034. Decisiones menores

**Estado:** Aceptada. Reúne los antiguos ADR 0005, 0010, 0015, 0020, 0023 y 0031, de una sola decisión cada uno.

- **Carpetas abiertas por bóveda** (antes 0005). Las carpetas abiertas del árbol se guardan por bóveda en `open_folders` de la configuración, para que cada bóveda recupere su árbol como lo dejó.
- **Cerrar es cerrar** (antes 0010). Cerrar la última ventana termina el proceso (ADR 0039). El usuario rechazó ocultarla en lugar de cerrar, porque costaría ~240 MB permanentes.
- **Unir los lados sin recargar** (antes 0015). Al volver de la vista dividida a un lado, las pestañas de la derecha pasan a la izquierda y sus webviews cambian de contenedor sin recargarse: las páginas conservan su estado. El divisor se guarda como proporción del ancho (`split_ratio`).
- **Ctrl+| además de Ctrl+\\ para dividir** (antes 0020). Con el teclado latinoamericano del usuario, «\\» necesita AltGr. Los dos atajos hacen lo mismo.
- **Pantalla completa** (antes 0023). Oculta el panel, las pestañas, la barra y el otro lado. Al salir, restaura el panel solo si estaba visible.
- **Diagramas con Pele** (antes 0031, «Mermaid completo»; cambiado el 2026-10-18, #40). Los bloques ```mermaid se dibujan con [Pele](https://github.com/obsidianmd/pele) (MIT, 205 KB con brotli), que lee la misma sintaxis y cubre los 33 tipos de Mermaid 12.1.0, mapas mentales incluidos. Antes se incluía Mermaid completo (1,18 MB con brotli). Medido en sway sin pantalla: el binario pesa 0,97 MB menos, `ondas.md` arranca en ~430 ms frente a ~700 ms y ocupa ~60 MB menos. Se usa `mount()` (zoom y ajuste al ancho); Escape restablece el zoom sin cerrar el panel de anotaciones. Enlaces e imágenes, con los valores de Pele: no añaden riesgo frente a los de una nota, y Rust decide igual qué hacer con cada clic. Las anotaciones de elemento hechas sobre un diagrama de Mermaid no encuentran su sitio (cambian la estructura y los ids del SVG).
