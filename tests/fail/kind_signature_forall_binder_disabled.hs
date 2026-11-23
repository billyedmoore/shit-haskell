{-# LANGUAGE NoKindSignatures #-}

import Data.Kind (Type)

identity :: forall (a :: Type). a -> a
identity = id

main :: IO ()
main = pure ()
