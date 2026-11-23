{-# LANGUAGE PolyKinds,TypeFamilies,TypeAbstractions,NoTypeApplications,CUSKs #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
class C (a :: Type) where
  type T @k a
main :: IO ()
main = pure ()
