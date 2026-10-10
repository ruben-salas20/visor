//! What agents use to drive visor: the command line, the socket server the
//! app listens on, and the MCP server, all speaking the same `Command`.
pub mod cli;
pub mod command;
pub mod mcp;
pub mod server;
