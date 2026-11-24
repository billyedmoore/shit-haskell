{-# LANGUAGE PolyKinds,TypeFamilies,TypeAbstractions,NoTypeApplications,CUSKs #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
newtype T @(k :: Type) (a :: k) = T (Proxy a)
main :: IO ()
main = pure ()
