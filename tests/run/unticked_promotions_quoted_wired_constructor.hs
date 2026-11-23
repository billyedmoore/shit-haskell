{-# LANGUAGE NoDataKinds, TemplateHaskell #-}

import GHC.Exts (RuntimeRep(IntRep))
import Language.Haskell.TH (Q, Type)

value :: Q Type
value = [t| 'IntRep |]

main :: IO ()
main = pure ()
