{-# LANGUAGE TypeFamilies, NoKindSignatures #-}

import Data.Kind (Type)

type family F a :: Type

data family G a :: Type
data instance G Int :: Type where
  Mk :: G Int

main :: IO ()
main = pure ()
