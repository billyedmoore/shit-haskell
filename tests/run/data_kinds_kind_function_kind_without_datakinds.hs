{-# LANGUAGE NoDataKinds #-}

import Data.Kind (Type)

data T (f :: Type -> Type) = T

main :: IO ()
main = pure ()
