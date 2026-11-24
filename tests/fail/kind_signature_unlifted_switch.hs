{-# LANGUAGE Haskell2010, UnliftedDatatypes #-}

import Data.Kind (Type)

data T (a :: Type) = Mk

main :: IO ()
main = pure ()
