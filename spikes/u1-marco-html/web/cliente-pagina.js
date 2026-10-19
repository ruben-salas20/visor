// Se carga en cada nota de la bóveda. Solo informa a Rust; Rust decide qué pasa.
const enviar = (m) => window.ipc.postMessage(JSON.stringify(m));
enviar({ accion: 'cargada', ruta: decodeURIComponent(location.pathname.replace('/boveda/', '')) });
document.addEventListener('keydown', (e) => {
  if (e.ctrlKey && e.key.toLowerCase() === 'w') {
    e.preventDefault();
    enviar({ accion: 'cerrar_pestana' });
  }
});
document.addEventListener('dragover', (e) => e.preventDefault());
document.addEventListener('drop', (e) => {
  e.preventDefault();
  enviar({ accion: 'soltar_en_pagina', ruta: e.dataTransfer.getData('text/plain') });
});
