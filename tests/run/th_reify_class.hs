-- Template Haskell `reify` of classes and `reifyInstances` (th-orphans' reifyManyWithoutInstances
-- reifies ''Lift and asks for its instances).
{-# LANGUAGE TemplateHaskell #-}
import Language.Haskell.TH
import Cls

main :: IO ()
main = do
  putStrLn $(do
    ClassI (ClassD sups n tvs fds sigs) insts <- reify ''Pretty
    let sigNames = [nameBase m | SigD m _ <- sigs]
    pure (LitE (StringL (show (nameBase n, length sups, length tvs, length fds, sigNames, length insts)))))
  putStrLn $(do
    ClassI (ClassD _ _ tvs fds _) _ <- reify ''Convert
    pure (LitE (StringL (show (length tvs, length fds)))))
  putStrLn $(do
    a <- reifyInstances ''Pretty [ConT ''Int]
    b <- reifyInstances ''Pretty [ConT ''Char]
    c <- reifyInstances ''Pretty [AppT ListT (ConT ''Char)]
    d <- isInstance ''Pretty [ConT ''Bool]
    e <- isInstance ''Pretty [AppT (ConT ''Box) (ConT ''Int)]
    f <- isInstance ''Pretty [AppT (ConT ''[]) (ConT ''Int)]
    g <- isInstance ''Show [ConT ''String]
    h <- isInstance ''Eq [AppT (AppT (TupleT 2) (ConT ''Int)) (ConT ''Bool)]
    pure (LitE (StringL (show (length a, length b, length c, d, e, f, g, h)))))
  putStrLn $(do
    ClassI (ClassD _ _ [KindedTV _ _ k] _ _) _ <- reify ''Functor
    pure (LitE (StringL (show k))))
