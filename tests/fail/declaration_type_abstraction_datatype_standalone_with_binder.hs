{-# LANGUAGE TypeAbstractions,StandaloneKindSignatures #-}
import Data.Kind (Type)
type T @k :: Type
data T = T
main :: IO ()
main = pure ()
