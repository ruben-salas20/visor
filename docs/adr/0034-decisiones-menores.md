# 0034. Decisiones menores

**Estado:** Aceptada. Reúne los antiguos ADR 0005, 0010, 0015, 0020, 0023 y 0031, de una sola decisión cada uno.

- **Carpetas abiertas por bóveda** (antes 0005). Las carpetas abiertas del árbol se guardan por bóveda en `open_folders` de la configuración, para que cada bóveda recupere su árbol como lo dejó.
- **Cerrar es cerrar** (antes 0010). Cerrar la ventana termina el proceso. El usuario rechazó ocultarla en lugar de cerrar, porque costaría ~240 MB permanentes.
- **Unir los lados sin recargar** (antes 0015). Al volver de la vista dividida a un lado, las pestañas de la derecha pasan a la izquierda y sus webviews cambian de contenedor sin recargarse: las páginas conservan su estado. El divisor se guarda como proporción del ancho (`split_ratio`).
- **Ctrl+| además de Ctrl+\\ para dividir** (antes 0020). Con el teclado latinoamericano del usuario, «\\» necesita AltGr. Los dos atajos hacen lo mismo.
- **Pantalla completa** (antes 0023). Oculta el panel, las pestañas, la barra y el otro lado. Al salir, restaura el panel solo si estaba visible.
- **Mermaid completo** (antes 0031). Se incluye Mermaid completo (1,15 MB comprimido). La versión *tiny* ahorra 0,6 MB pero pierde los mapas mentales; el usuario eligió el completo.
