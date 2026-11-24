{-# LANGUAGE NoDataKinds #-}

import Data.Kind (Type)
import GHC.Exts (RuntimeRep)

data T (x :: Type) (r :: RuntimeRep) = T

main :: IO ()
main = pure ()
