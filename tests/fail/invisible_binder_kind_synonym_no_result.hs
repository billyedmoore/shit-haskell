{-# LANGUAGE PolyKinds,TypeFamilies,TypeAbstractions,NoTypeApplications,CUSKs #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
type T @(k :: Type) (a :: k) = Proxy a
main :: IO ()
main = pure ()
