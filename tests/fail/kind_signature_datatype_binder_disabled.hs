{-# LANGUAGE NoKindSignatures #-}

import Data.Kind (Type)

data T (a :: Type) = Mk a

main :: IO ()
main = pure ()
