use std::{fs::File, io::BufReader, path::Path};

use anyhow::anyhow;
use pdb::{AddressMap, FallibleIterator, SymbolTable, PDB};
use rustc_demangle::demangle;

pub struct LoadedPdb<'s> {
    _pdb: PDB<'s, BufReader<File>>,
    symbol_table: SymbolTable<'s>,
    address_map: AddressMap<'s>,
}

pub fn load_pdb<'p>(path: &'p Path) -> anyhow::Result<LoadedPdb<'p>> {
    let file = BufReader::new(File::open(path)?);

    let mut pdb = pdb::PDB::open(file)?;

    let symbol_table = pdb.global_symbols()?;
    let address_map = pdb.address_map()?;

    Ok(LoadedPdb { _pdb: pdb, symbol_table, address_map })
}

pub fn find_offset<'s>(mut predicate: impl FnMut(&str) -> bool, pdb: &LoadedPdb<'s>) -> anyhow::Result<Option<isize>> {
    let mut symbols = pdb.symbol_table.iter();

    while let Some(symbol) = symbols.next()? {
        match symbol.parse() {
            Ok(pdb::SymbolData::Public(data)) if data.function => {
                let name = demangle(&data.name.to_string()).to_string();

                if predicate(&name) {
                    return Ok(Some(data.offset.to_rva(&pdb.address_map).ok_or(anyhow!("could not compute offset"))?.0 as isize));
                }
            },
            _ => {},
        }
    }

    Ok(None)
}
