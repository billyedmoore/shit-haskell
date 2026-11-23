{-# LANGUAGE NoDataKinds, StandaloneKindSignatures #-}

import Data.Kind (Type)

type D :: [Type] -> Type
data D a = D

main :: IO ()
main = pure ()
