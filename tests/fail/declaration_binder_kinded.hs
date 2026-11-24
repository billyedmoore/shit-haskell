{-# LANGUAGE KindSignatures #-}
import Data.Kind (Type)
data Broken (a :: Type) (a :: Type) = Broken
main :: IO ()
main = pure ()
