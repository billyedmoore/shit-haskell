module Control.Exception (
    SomeException(..), Exception(..), ErrorCall(..), ArithException(..), AsyncException(..), IOException,
    throw, throwIO, catch, handle, try, evaluate, finally, onException, bracket, bracket_,
    catches, Handler(..), ioError, assert, mask_
  ) where

import Base

-- An untyped payload. Coercions are identity at run time.
data Any

foreign import prim "unsafeCoerce" unsafeCoerce :: a -> b
foreign import prim "throwExc" primThrow :: SomeException -> a
foreign import prim "catchExc" primCatch :: IO a -> (String -> SomeException) -> (SomeException -> IO a) -> World -> IORes a
foreign import prim "throwIOExc" primThrowIO :: SomeException -> World -> IORes a

-- The type tag, the payload, and the text shown for the exception.
data SomeException = SomeException String Any String

instance Show SomeException where
  showsPrec _ (SomeException _ _ s) = showString s

class Show e => Exception e where
  toException :: e -> SomeException
  fromException :: SomeException -> Maybe e
  displayException :: e -> String
  exceptionTypeTag :: e -> String
  toException e = SomeException (exceptionTypeTag e) (unsafeCoerce e) (show e)
  fromException (SomeException tag payload _) = fromCoerced tag (unsafeCoerce payload)
  displayException e = show e

-- The payload is cast to the expected type after its tag has been compared.
fromCoerced :: Exception e => String -> e -> Maybe e
fromCoerced tag r = if tag == exceptionTypeTag r then Just r else Nothing

instance Exception SomeException where
  toException se = se
  fromException se = Just se
  exceptionTypeTag _ = "SomeException"

data ErrorCall = ErrorCall String | ErrorCallWithLocation String String

instance Show ErrorCall where
  showsPrec _ (ErrorCall m) = showString m
  showsPrec _ (ErrorCallWithLocation m _) = showString m

instance Eq ErrorCall where
  a == b = show a == show b

instance Exception ErrorCall

data ArithException = Overflow | Underflow | LossOfPrecision | DivideByZero | Denormal | RatioZeroDenominator

instance Show ArithException where
  showsPrec _ Overflow = showString "arithmetic overflow"
  showsPrec _ Underflow = showString "arithmetic underflow"
  showsPrec _ LossOfPrecision = showString "loss of precision"
  showsPrec _ DivideByZero = showString "divide by zero"
  showsPrec _ Denormal = showString "denormal"
  showsPrec _ RatioZeroDenominator = showString "Ratio has zero denominator"

instance Eq ArithException where
  a == b = show a == show b

instance Exception ArithException

data AsyncException = StackOverflow | HeapOverflow | ThreadKilled | UserInterrupt

instance Show AsyncException where
  showsPrec _ StackOverflow = showString "stack overflow"
  showsPrec _ HeapOverflow = showString "heap overflow"
  showsPrec _ ThreadKilled = showString "thread killed"
  showsPrec _ UserInterrupt = showString "user interrupt"

instance Eq AsyncException where
  a == b = show a == show b

instance Exception AsyncException

type IOException = IOError

instance Exception IOError

throw :: Exception e => e -> a
throw e = primThrow (toException e)

throwIO :: Exception e => e -> IO a
throwIO e = IO (primThrowIO (toException e))

-- Runtime errors reach handlers as ErrorCall, or DivideByZero for arithmetic.
runtimeError :: String -> SomeException
runtimeError msg
  | msg == "divide by zero" = toException DivideByZero
  | isIOMessage msg = toException (IOError msg)
  | otherwise = toException (ErrorCall msg)

-- I/O failures from primitives are reported with GHC's "openFile:" wording.
isIOMessage :: String -> Bool
isIOMessage s = any (\t -> take 9 t == "openFile:") (tailsOf s)
  where
    tailsOf [] = [[]]
    tailsOf xs@(_ : rest) = xs : tailsOf rest

catch :: Exception e => IO a -> (e -> IO a) -> IO a
catch act h = IO (primCatch act runtimeError (\se -> case fromException se of
  Just e -> h e
  Nothing -> IO (primThrowIO se)))

handle :: Exception e => (e -> IO a) -> IO a -> IO a
handle h act = catch act h

try :: Exception e => IO a -> IO (Either e a)
try act = catch (act >>= \v -> return (Right v)) (\e -> return (Left e))

evaluate :: a -> IO a
evaluate x = IO (\_ -> x `seq` IORes x)

onException :: IO a -> IO b -> IO a
onException io what = catch io (\e -> what >> throwIO (e :: SomeException))

finally :: IO a -> IO b -> IO a
finally a sequel = do
  r <- onException a sequel
  _ <- sequel
  return r

bracket :: IO a -> (a -> IO b) -> (a -> IO c) -> IO c
bracket before after thing = do
  a <- before
  r <- onException (thing a) (after a)
  _ <- after a
  return r

bracket_ :: IO a -> IO b -> IO c -> IO c
bracket_ before after thing = bracket before (const after) (const thing)

data Handler a = Handler (SomeException -> Maybe (IO a))

catches :: IO a -> [Handler a] -> IO a
catches act handlers = catch act (\se -> firstHandler handlers se)
  where
    firstHandler [] se = throwIO se
    firstHandler (Handler h : hs) se = case h se of
      Just io -> io
      Nothing -> firstHandler hs se

assert :: Bool -> a -> a
assert _ x = x

mask_ :: IO a -> IO a
mask_ io = io
