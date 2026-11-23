{-# LANGUAGE StandaloneKindSignatures, NoKindSignatures #-}

import Data.Kind (Type)

type T :: Type
data T = Mk

main :: IO ()
main = pure ()
