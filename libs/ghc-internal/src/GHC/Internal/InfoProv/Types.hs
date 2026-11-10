{-# LANGUAGE NoImplicitPrelude #-}

-- hsc stand-in for GHC's InfoProv/Types.hsc. That module reads the RTS's info-table provenance
-- records (which closure came from which source location). hsc has no info tables, so no lookup
-- finds an entry: the types and names are the real ones, and the lookups report none.
module GHC.Internal.InfoProv.Types
    ( InfoProv(..)
    , ipLoc
    , ipeProv
    , InfoProvEnt
    , peekInfoProv
    , getIPE
    , StgInfoTable
    , lookupIPE
    ) where

import GHC.Internal.Base
import GHC.Internal.Ptr (Ptr, castPtr)
import GHC.Internal.ClosureTypes
import GHC.Internal.Show (Show)

data InfoProv = InfoProv {
  ipName :: String,
  ipDesc :: ClosureType,
  ipTyDesc :: String,
  ipLabel :: String,
  ipUnitId :: String,
  ipMod :: String,
  ipSrcFile :: String,
  ipSrcSpan :: String
} deriving (Eq, Show)

ipLoc :: InfoProv -> String
ipLoc ipe = ipSrcFile ipe ++ ":" ++ ipSrcSpan ipe

data InfoProvEnt
data StgInfoTable

ipeProv :: Ptr InfoProvEnt -> Ptr InfoProv
ipeProv p = castPtr p

peekInfoProv :: Ptr InfoProv -> IO InfoProv
peekInfoProv _ = errorWithoutStackTrace "info-table provenance is not available in hsc"

lookupIPE :: Ptr StgInfoTable -> IO (Maybe InfoProv)
lookupIPE _ = return Nothing

getIPE :: a -> r -> (Ptr InfoProvEnt -> IO r) -> IO r
getIPE _ fail _ = return fail
