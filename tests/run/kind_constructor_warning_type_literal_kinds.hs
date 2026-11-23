{-# LANGUAGE NoDataKinds #-}

import GHC.TypeLits (Nat, Symbol)

data T (n :: Nat) (s :: Symbol) = T

main :: IO ()
main = pure ()
