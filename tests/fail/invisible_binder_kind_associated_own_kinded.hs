{-# LANGUAGE PolyKinds,TypeFamilies,TypeAbstractions,NoTypeApplications,CUSKs #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
class C a where
  type T @(k :: Type) (b :: k) a
main :: IO ()
main = pure ()
