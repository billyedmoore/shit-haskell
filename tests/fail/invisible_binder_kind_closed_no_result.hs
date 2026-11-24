{-# LANGUAGE PolyKinds,TypeFamilies,TypeAbstractions,NoTypeApplications,CUSKs #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
type family T @(k :: Type) (a :: k) where
  T a = Bool
main :: IO ()
main = pure ()
