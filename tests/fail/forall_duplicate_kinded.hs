{-# LANGUAGE PolyKinds #-}

import Data.Kind (Type)

f :: forall (a :: Type) (a :: Type). a -> a
f = id

main :: IO ()
main = pure ()
