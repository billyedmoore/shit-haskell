{-# LANGUAGE NoDataKinds, StandaloneKindSignatures #-}

import Data.Kind (Type)
data Mark = MkMark
type T :: Mark -> Type
data T x = T

main :: IO ()
main = pure ()
